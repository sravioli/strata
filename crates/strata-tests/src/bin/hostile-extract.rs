//! A test program in place of `strata-extract` (spec extraction-sandbox, task 3.3). The L1 tests
//! run it in the sandbox of the worker. The first word of standard input selects one behaviour.
//! It prints what it finds, one fact on each line.

use std::io::{Read, Write};

fn main() {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let mut words = input.split_whitespace();
    let out = &mut std::io::stdout();
    match words.next().unwrap_or("") {
        // TC-7-01
        "network" => {
            for addr in ["1.1.1.1:80", "127.0.0.1:5432", "[::1]:5432"] {
                let r = std::net::TcpStream::connect(addr);
                let _ = writeln!(
                    out,
                    "connect {addr} {}",
                    if r.is_ok() { "ok" } else { "error" }
                );
            }
        }
        // TC-7-02: the environment, and each path that the test names (files of the worker).
        "environment" => {
            // The names only: a value could be a secret.
            let names: Vec<_> = std::env::vars_os()
                .map(|(k, _)| k.to_string_lossy().into_owned())
                .collect();
            let _ = writeln!(out, "variables {}", names.join(","));
            let root: Vec<_> = std::fs::read_dir("/")
                .map(|d| {
                    d.flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            let _ = writeln!(out, "root {}", root.join(","));
            for path in words {
                let r = std::fs::read(path);
                let _ = writeln!(
                    out,
                    "read {path} {}",
                    if r.is_ok() { "ok" } else { "error" }
                );
            }
            let write = std::fs::write("/new-file", b"x");
            let _ = writeln!(out, "write {}", if write.is_ok() { "ok" } else { "error" });
        }
        // TC-7-05: each page is written, so the memory is used.
        "memory" => {
            let v = vec![1u8; 1536 * 1024 * 1024];
            let _ = writeln!(out, "{}", v.iter().map(|&b| b as u64).sum::<u64>());
        }
        // The scenario "The kernel stops the child of the process for memory": the child uses
        // more than 1 GB, and this process exits with 0.
        "memory-child" => {
            let child = std::process::Command::new("/strata-extract")
                .stdin(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut c| {
                    c.stdin.take().unwrap().write_all(b"memory")?;
                    c.wait()
                });
            let _ = writeln!(out, "child {child:?}");
        }
        "forever" => loop {
            std::hint::spin_loop();
        },
        // The process is PID 1 in the sandbox: abort() ends it on a signal.
        "crash" => std::process::abort(),
        // TC-7-06: more than 50 MB.
        "output" => {
            let block = vec![b'x'; 1024 * 1024];
            for _ in 0..60 {
                if out.write_all(&block).is_err() {
                    break;
                }
            }
        }
        // TC-7-13: output that fails the check, with exit code 0.
        "invalid" => {
            let _ = out.write_all(b"not json");
        }
        // TC-49-10
        "io_uring" => {
            for (name, nr) in [
                ("io_uring_setup", libc::SYS_io_uring_setup),
                ("io_uring_enter", libc::SYS_io_uring_enter),
                ("io_uring_register", libc::SYS_io_uring_register),
            ] {
                let mut params = [0u8; 120];
                // SAFETY: a raw system call with a buffer that lives for the call; the test only
                // reads the result. A kernel that permits the call writes at most 120 bytes.
                let r = unsafe { libc::syscall(nr, 4, params.as_mut_ptr(), 0, 0, 0, 0) };
                let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
                let _ = writeln!(out, "{name} {r} {errno}");
            }
        }
        // A copy of the output of strata-extract: the test of the sandbox itself.
        _ => {
            let _ = out.write_all(br#"{"format":"pdf","pages":1,"blocks":[{"kind":"page","index":1,"text":"hostile"}]}"#);
        }
    }
    let _ = out.flush();
}
