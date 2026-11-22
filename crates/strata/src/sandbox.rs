//! The sandbox of tech design 0007: one run of `strata-extract` in nsjail, in its own child
//! cgroup of a delegated cgroup (`STRATA_CGROUP`), with the limits of 0007.
//!
//! The control group limits work with no root only in a delegated cgroup that holds no process
//! (#49, task 1.2). nsjail exits with 255 when it cannot start the sandbox, and with 128 + the
//! signal when the process stops on a signal.

use std::ffi::OsString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

pub const NSJAIL: &str = "STRATA_NSJAIL";
pub const EXTRACT: &str = "STRATA_EXTRACT";
pub const CGROUP: &str = "STRATA_CGROUP";

/// `pdftotext` of the system (`poppler-utils`). The image of the worker pins it (#43).
pub const PDFTOTEXT: &str = "/usr/bin/pdftotext";

/// The calls that fail with `EPERM` (owner, 2026-11-22). The list allows each other call: #103
/// owns the allowlist of 0007. seccomp does not see the operations of an io_uring ring, so the
/// ring calls are on the list (TC-49-10). kafel names `umount2` of x86_64 `umount`.
pub const SECCOMP: &str = "ERRNO(1) { io_uring_setup, io_uring_enter, io_uring_register, \
     ptrace, bpf, mount, umount, pivot_root, unshare, setns, keyctl, add_key, request_key, \
     perf_event_open, userfaultfd, process_vm_readv, process_vm_writev, init_module, \
     finit_module, delete_module, kexec_load, kexec_file_load, \
     socket(domain, type, protocol) { domain != 1 } } DEFAULT ALLOW";

/// The limits of one extraction. `LIMITS` holds the values of 0007. The L1 tests use shorter
/// time limits, so that a test does not wait 120 seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub memory: u64,
    pub cpu: Duration,
    pub wall: Duration,
    pub output: u64,
    pub processes: u32,
}

pub const LIMITS: Limits = Limits {
    memory: 1024 * 1024 * 1024,
    cpu: Duration::from_secs(60),
    wall: Duration::from_secs(120),
    output: strata_extract::MAX_OUTPUT,
    processes: 2,
};

/// How a run ended, in the order of the checks of the worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// `oom_kill` above 0 in the cgroup of the run, whatever the exit code.
    Memory,
    /// The worker stopped the run at the output limit.
    OutputTooLarge,
    /// The worker stopped the run at the wall-clock limit, or nsjail or `RLIMIT_CPU` did (exit 137).
    Timeout,
    /// The caller stopped the run: the lease of the job is lost. Nothing writes the output.
    Cancelled,
    /// nsjail could not start the sandbox (exit 255). The cause is not in the file.
    SandboxFailure,
    /// The process stopped on a signal other than `SIGKILL`.
    Crash,
    /// The exit code of `strata-extract`, below 128.
    Exit(i32),
}

pub struct Run {
    pub stop: Stop,
    /// At most the output limit and one byte of standard output.
    pub output: Vec<u8>,
}

/// The sandbox of one worker process.
pub struct Sandbox {
    nsjail: PathBuf,
    extract: PathBuf,
    cgroup: PathBuf,
    limits: Limits,
}

impl Sandbox {
    /// The sandbox from the three variables, with the limits of 0007.
    pub fn from_env() -> Result<Self, String> {
        Self::from_vars(|k| std::env::var(k).ok(), LIMITS)
    }

    /// The sandbox from the values that `var` gives. Each error names its variable. The cgroup
    /// gets the controllers `memory`, `pids` and `cpu` for its children: the write fails when the
    /// cgroup is not delegated to the user, or holds a process.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>, limits: Limits) -> Result<Self, String> {
        let path = |name: &str| -> Result<PathBuf, String> {
            let value = var(name)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| format!("set {name}"))?;
            let p = PathBuf::from(value);
            if p.exists() {
                Ok(p)
            } else {
                Err(format!("{name}: {} does not exist", p.display()))
            }
        };
        let (nsjail, extract, cgroup) = (path(NSJAIL)?, path(EXTRACT)?, path(CGROUP)?);
        // `write` with no `create`: a folder that is not a cgroup gets no new file.
        std::fs::OpenOptions::new()
            .write(true)
            .open(cgroup.join("cgroup.subtree_control"))
            .and_then(|mut f| std::io::Write::write_all(&mut f, b"+memory +pids +cpu"))
            .map_err(|e| {
                format!(
                    "{CGROUP}: {} is not an empty delegated cgroup: {e}",
                    cgroup.display()
                )
            })?;
        Ok(Sandbox {
            nsjail,
            extract,
            cgroup,
            limits,
        })
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// The arguments of nsjail for one run in the cgroup `run`.
    fn args(&self, run: &Path) -> Vec<OsString> {
        let l = &self.limits;
        let mib = |b: u64| (b / (1024 * 1024)).max(1).to_string();
        let mut a: Vec<OsString> = Vec::new();
        let mut push = |s: &str| a.push(s.into());
        for s in ["-Mo", "-Q", "--use_cgroupv2", "--disable_proc"] {
            push(s);
        }
        let values = [
            ("--cgroup_mem_max", l.memory.to_string()),
            ("--cgroup_mem_swap_max", "0".into()),
            ("--cgroup_pids_max", l.processes.to_string()),
            ("--cgroup_cpu_ms_per_sec", "1000".into()),
            // Above the cgroup limit, so that the cgroup stops the process and the class is
            // `ExtractionMemory`, not a failed allocation.
            ("--rlimit_as", mib(l.memory.saturating_mul(4))),
            ("--rlimit_cpu", l.cpu.as_secs().max(1).to_string()),
            ("--rlimit_fsize", "0".into()),
            ("--rlimit_nofile", "32".into()),
            ("--rlimit_core", "0".into()),
            // nsjail stops the process also when the worker stopped (TC-7-10).
            ("--time_limit", (l.wall.as_secs() + 1).to_string()),
            ("--seccomp_string", SECCOMP.into()),
        ];
        for (k, v) in values {
            push(k);
            push(&v);
        }
        a.push("--cgroupv2_mount".into());
        a.push(run.into());
        // A read-only root with only the binaries and their libraries: no file of the worker.
        for dir in ["/lib", "/lib64", "/usr/lib", "/usr/lib64"] {
            let p = Path::new(dir);
            match std::fs::read_link(p) {
                Ok(target) => {
                    a.push("--symlink".into());
                    a.push(format!("{}:{dir}", target.display()).into());
                }
                Err(_) if p.is_dir() => {
                    a.push("--bindmount_ro".into());
                    a.push(dir.into());
                }
                Err(_) => {}
            }
        }
        for file in ["/etc/ld.so.cache", "/usr/share/poppler", PDFTOTEXT] {
            if Path::new(file).exists() {
                a.push("--bindmount_ro".into());
                a.push(file.into());
            }
        }
        let mut extract = OsString::from(self.extract.as_os_str());
        extract.push(":/strata-extract");
        a.extend([
            "--bindmount_ro".into(),
            extract,
            "--".into(),
            "/strata-extract".into(),
        ]);
        a
    }

    /// Runs `strata-extract` with `input` on its standard input. The run stops at the limits, or
    /// when `cancel` ends.
    pub async fn run(&self, input: &[u8], cancel: impl Future<Output = ()>) -> Run {
        static N: AtomicU64 = AtomicU64::new(0);
        let run = self.cgroup.join(format!(
            "run-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        if let Err(e) = std::fs::create_dir(&run) {
            tracing::error!(error = %e, "the cgroup of a run");
            return Run {
                stop: Stop::SandboxFailure,
                output: Vec::new(),
            };
        }
        let result = self.run_in(&run, input, cancel).await;
        remove_cgroup(&run).await;
        result
    }

    async fn run_in(&self, run: &Path, input: &[u8], cancel: impl Future<Output = ()>) -> Run {
        let child = Command::new(&self.nsjail)
            .args(self.args(run))
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // nsjail and the process share this descriptor, so it can hold tenant content.
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "nsjail does not start");
                return Run {
                    stop: Stop::SandboxFailure,
                    output: Vec::new(),
                };
            }
        };
        let (mut stdin, mut stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
        let limit = usize::try_from(self.limits.output)
            .unwrap_or(usize::MAX)
            .saturating_add(1);
        let mut output = Vec::new();
        let mut too_large = false;
        // A process can write without a read of its input: the write and the read go together.
        let write = async {
            // A process that stops early closes the pipe: that is not the result.
            let _ = stdin.write_all(input).await;
            drop(stdin);
        };
        let read = async {
            let mut buf = vec![0; 64 * 1024];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        output.extend_from_slice(&buf[..n.min(limit - output.len())]);
                        if output.len() >= limit {
                            too_large = true;
                            kill(run);
                            break;
                        }
                    }
                }
            }
        };
        let mut io = Box::pin(async { tokio::join!(write, read) });
        let deadline = tokio::time::Instant::now() + self.limits.wall;
        tokio::pin!(cancel);
        let mut stopped = None;
        let mut status = None;
        tokio::select! {
            _ = &mut io => {}
            () = tokio::time::sleep_until(deadline) => stopped = Some(Stop::Timeout),
            () = &mut cancel => stopped = Some(Stop::Cancelled),
        }
        drop(io);
        if stopped.is_none() {
            // The process can close its output and go on: the wait has the same limits.
            tokio::select! {
                s = child.wait() => status = Some(s),
                () = tokio::time::sleep_until(deadline) => stopped = Some(Stop::Timeout),
                () = &mut cancel => stopped = Some(Stop::Cancelled),
            }
        }
        let status = match status {
            Some(s) => s,
            None => {
                kill(run);
                // nsjail ends when its process ends. A stop of nsjail itself would leave its cgroup.
                match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
                    Ok(s) => s,
                    Err(_) => {
                        let _ = child.start_kill();
                        child.wait().await
                    }
                }
            }
        };
        let oom = oom_kills(run).await > 0;
        let stop = match (oom, too_large, stopped, status.map(|s| s.code())) {
            (true, ..) => Stop::Memory,
            (_, true, ..) => Stop::OutputTooLarge,
            (_, _, Some(s), _) => s,
            (.., Ok(Some(255))) | (.., Err(_)) => Stop::SandboxFailure,
            (.., Ok(Some(137))) => Stop::Timeout,
            (.., Ok(Some(c))) if c >= 128 => Stop::Crash,
            (.., Ok(Some(c))) => Stop::Exit(c),
            // nsjail itself stopped on a signal.
            (.., Ok(None)) => Stop::SandboxFailure,
        };
        Run { stop, output }
    }
}

/// Stops each process in the cgroup of the run (cgroup v2, `cgroup.kill`).
fn kill(run: &Path) {
    if let Err(e) = std::fs::write(run.join("cgroup.kill"), "1") {
        tracing::error!(error = %e, "cgroup.kill");
    }
}

/// The count of processes that the kernel stopped for memory in the cgroup of the run and its
/// children. The count stays after nsjail removes its child cgroup.
async fn oom_kills(run: &Path) -> u64 {
    // The kernel can count the stop a short time after the process ends.
    for _ in 0..50 {
        let events = std::fs::read_to_string(run.join("cgroup.events")).unwrap_or_default();
        if events.lines().any(|l| l == "populated 0") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    std::fs::read_to_string(run.join("memory.events"))
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("oom_kill ")?.parse().ok())
        .unwrap_or(0)
}

/// Removes the cgroup of the run and the child cgroups that nsjail left.
async fn remove_cgroup(run: &Path) {
    for _ in 0..50 {
        if let Ok(entries) = std::fs::read_dir(run) {
            for e in entries.flatten() {
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    let _ = std::fs::remove_dir(e.path());
                }
            }
        }
        if std::fs::remove_dir(run).is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tracing::error!(cgroup = %run.display(), "the cgroup of a run stays");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&'static str, String)]) -> impl Fn(&str) -> Option<String> {
        let pairs = pairs.to_vec();
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone())
    }

    #[test]
    fn each_missing_setting_is_named() {
        let here = std::env::current_dir().unwrap().display().to_string();
        let all = [
            (NSJAIL, here.clone()),
            (EXTRACT, here.clone()),
            (CGROUP, "/nonexistent/cgroup".into()),
        ];
        for name in [NSJAIL, EXTRACT, CGROUP] {
            let some: Vec<_> = all.iter().filter(|(n, _)| *n != name).cloned().collect();
            let e = Sandbox::from_vars(vars(&some), LIMITS).err().unwrap();
            assert!(e.contains(name), "{e}");
        }
        let e = Sandbox::from_vars(vars(&all), LIMITS).err().unwrap();
        assert!(e.starts_with(CGROUP) && e.contains("does not exist"), "{e}");
        let not_cgroup = [
            (NSJAIL, here.clone()),
            (EXTRACT, here.clone()),
            (CGROUP, here),
        ];
        let e = Sandbox::from_vars(vars(&not_cgroup), LIMITS).err().unwrap();
        assert!(e.contains("not an empty delegated cgroup"), "{e}");
    }

    #[test]
    fn the_arguments_hold_the_limits_and_no_root_of_the_worker() {
        let s = Sandbox {
            nsjail: "/n".into(),
            extract: "/x/strata-extract".into(),
            cgroup: "/c".into(),
            limits: LIMITS,
        };
        let a: Vec<String> = s
            .args(Path::new("/c/run"))
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        let joined = a.join(" ");
        for part in [
            "--cgroup_mem_max 1073741824",
            "--cgroup_pids_max 2",
            "--rlimit_cpu 60",
            "--rlimit_fsize 0",
            "--time_limit 121",
            "--cgroupv2_mount /c/run",
            "--bindmount_ro /x/strata-extract:/strata-extract -- /strata-extract",
        ] {
            assert!(joined.contains(part), "{part}: {joined}");
        }
        assert!(SECCOMP.contains("io_uring_setup, io_uring_enter, io_uring_register"));
        assert!(
            !a.iter()
                .any(|x| x == "--chroot" || x == "--keep_env" || x == "-e")
        );
    }
}
