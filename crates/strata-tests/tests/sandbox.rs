//! L1 tests of the sandbox (spec extraction-sandbox): nsjail with the configuration of the
//! worker, around the real `strata-extract` or around the test program `hostile-extract`.
//! They run in the delegated cgroup of `scripts/in-cgroup.sh` (`make l1`).

mod common;

use std::time::Duration;

use common::*;
use strata::extraction::{Outcome, Part, outcome};
use strata::sandbox::{LIMITS, Limits, Run, Stop};
use strata_core::revision::FailureClass;

/// The limits of 0007, with a wall-clock limit that a test can wait for.
const SHORT: Limits = Limits {
    wall: Duration::from_secs(3),
    cpu: Duration::from_secs(3),
    ..LIMITS
};

async fn hostile_run(input: &str, limits: Limits) -> Run {
    sandbox(&hostile(), None, limits)
        .run(input.as_bytes(), std::future::pending())
        .await
}

fn lines(run: &Run) -> Vec<String> {
    String::from_utf8_lossy(&run.output)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Task 3.1: the PDF of two pages gives its JSON in the sandbox.
#[tokio::test]
async fn strata_extract_gives_the_text_of_a_pdf_in_the_sandbox() {
    let pdf = std::fs::read(format!(
        "{}/fixtures/two-pages.pdf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let run = sandbox(&strata_extract(), None, LIMITS)
        .run(&pdf, std::future::pending())
        .await;
    assert_eq!(
        run.stop,
        Stop::Exit(0),
        "{}",
        String::from_utf8_lossy(&run.output)
    );
    assert_eq!(
        outcome(run),
        Outcome::Text(vec![
            Part {
                location: 1,
                text: "alpha page one".into()
            },
            Part {
                location: 2,
                text: "bravo page two".into()
            },
        ])
    );
}

/// TC-7-01
#[tokio::test]
async fn no_network() {
    let run = hostile_run("network", SHORT).await;
    assert_eq!(run.stop, Stop::Exit(0));
    let lines = lines(&run);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines.iter().all(|l| l.ends_with(" error")), "{lines:?}");
}

/// TC-7-02
#[tokio::test]
async fn no_environment_and_no_secrets() {
    let worker_file = format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"));
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let input =
        format!("environment {worker_file} {home} /etc/passwd /etc/shadow /proc/self/environ");
    let run = hostile_run(&input, SHORT).await;
    assert_eq!(run.stop, Stop::Exit(0));
    let lines = lines(&run);
    // The coverage runtime of an instrumented build sets this name in the process itself.
    let names: Vec<&str> = lines[0]
        .trim_start_matches("variables ")
        .split(',')
        .filter(|n| !n.is_empty() && *n != "__LLVM_PROFILE_RT_INIT_ONCE")
        .collect();
    assert!(names.is_empty(), "variables in the sandbox: {names:?}");
    let root: Vec<&str> = lines[1].trim_start_matches("root ").split(',').collect();
    for entry in &root {
        assert!(
            ["lib", "lib64", "usr", "etc", "strata-extract"].contains(entry),
            "{root:?}"
        );
    }
    let reads: Vec<_> = lines.iter().filter(|l| l.starts_with("read ")).collect();
    assert_eq!(reads.len(), 5, "{lines:?}");
    assert!(reads.iter().all(|l| l.ends_with(" error")), "{lines:?}");
    assert_eq!(lines.last().unwrap(), "write error");
}

/// TC-7-05
#[tokio::test]
async fn more_than_1_gb_of_memory() {
    let run = hostile_run(
        "memory",
        Limits {
            wall: Duration::from_secs(30),
            ..LIMITS
        },
    )
    .await;
    assert_eq!(run.stop, Stop::Memory);
    assert_eq!(
        outcome(run),
        Outcome::Failed(FailureClass::ExtractionMemory)
    );
}

/// Scenario "The kernel stops the child of the process for memory".
#[tokio::test]
async fn the_kernel_stops_the_child_of_the_process_for_memory() {
    // A cgroup of this test only, so that the test can see that the run leaves no cgroup.
    let base = std::path::PathBuf::from(std::env::var(strata::sandbox::CGROUP).expect("make l1"));
    let _ = sandbox(&hostile(), None, SHORT); // the controllers of the base cgroup
    let own = base.join(unique("test"));
    std::fs::create_dir(&own).unwrap();
    let s = sandbox(
        &hostile(),
        Some(&own),
        Limits {
            wall: Duration::from_secs(30),
            ..LIMITS
        },
    );
    let run = s.run(b"memory-child", std::future::pending()).await;
    let printed = String::from_utf8_lossy(&run.output).into_owned();
    assert_eq!(run.stop, Stop::Memory, "{printed}");
    let left: Vec<_> = std::fs::read_dir(&own)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();
    assert!(left.is_empty(), "{left:?}");
    std::fs::remove_dir(&own).unwrap();
}

/// TC-7-06
#[tokio::test]
async fn more_than_50_mb_of_output() {
    let run = hostile_run("output", SHORT).await;
    assert_eq!(run.stop, Stop::OutputTooLarge);
    assert_eq!(run.output.len() as u64, LIMITS.output + 1);
    assert_eq!(
        outcome(run),
        Outcome::Failed(FailureClass::ExtractionOutputTooLarge)
    );
}

/// TC-49-10
#[tokio::test]
async fn each_io_uring_call_fails() {
    let run = hostile_run("io_uring", SHORT).await;
    assert_eq!(run.stop, Stop::Exit(0));
    assert_eq!(
        lines(&run),
        [
            "io_uring_setup -1 1",
            "io_uring_enter -1 1",
            "io_uring_register -1 1"
        ]
    );
}

/// The wall-clock limit stops a process that runs with no end.
#[tokio::test]
async fn the_time_limit_stops_the_process() {
    let start = std::time::Instant::now();
    let run = hostile_run("forever", SHORT).await;
    assert_eq!(run.stop, Stop::Timeout);
    assert!(
        start.elapsed() < Duration::from_secs(15),
        "{:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn a_crash_and_output_that_fails_the_check() {
    let crash = hostile_run("crash", SHORT).await;
    assert_eq!(crash.stop, Stop::Crash);
    let invalid = hostile_run("invalid", SHORT).await;
    assert_eq!(
        outcome(invalid),
        Outcome::Failed(FailureClass::InvalidOutput)
    );
}

/// A run that the caller stops writes no result.
#[tokio::test]
async fn a_cancelled_run_stops() {
    let s = sandbox(
        &hostile(),
        None,
        Limits {
            wall: Duration::from_secs(60),
            ..LIMITS
        },
    );
    let run = s
        .run(b"forever", tokio::time::sleep(Duration::from_millis(300)))
        .await;
    assert_eq!(run.stop, Stop::Cancelled);
    assert_eq!(outcome(run), Outcome::Cancelled);
}

/// A binary that nsjail cannot start: a failure of the sandbox, not of the file.
#[tokio::test]
async fn nsjail_cannot_start_the_sandbox() {
    let not_a_program =
        std::path::PathBuf::from(format!("{}/fixtures/README.md", env!("CARGO_MANIFEST_DIR")));
    let run = sandbox(&not_a_program, None, SHORT)
        .run(b"x", std::future::pending())
        .await;
    assert_eq!(run.stop, Stop::SandboxFailure);
    assert_eq!(outcome(run), Outcome::SandboxFailure);
}
