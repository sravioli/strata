//! The subcommands read the database URL only from `STRATA_DATABASE_URL`.

#[test]
fn no_variable_gives_an_error_that_names_it() {
    for args in [&["migrate"][..], &["api"], &["admin", "tenant", "list"]] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_strata"))
            .args(args)
            .env_remove("STRATA_DATABASE_URL")
            .output()
            .unwrap();
        assert!(!out.status.success(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("STRATA_DATABASE_URL"),
            "{args:?}"
        );
    }
}

/// The storage settings of the spec document-upload: each error names its variable.
#[test]
fn api_stops_when_a_storage_setting_is_wrong() {
    let base = [
        (
            "STRATA_DATABASE_URL",
            "postgres://strata_api@localhost:6432/strata",
        ),
        ("AWS_ENDPOINT", "http://localhost:3900"),
        ("AWS_ACCESS_KEY_ID", "key"),
        ("AWS_SECRET_ACCESS_KEY", "secret"),
        ("AWS_DEFAULT_REGION", "garage"),
        ("AWS_BUCKET_NAME", "strata"),
        ("AWS_ALLOW_HTTP", "true"),
    ];
    // "No secret key", and "An http endpoint needs AWS_ALLOW_HTTP".
    for missing in ["AWS_SECRET_ACCESS_KEY", "AWS_ALLOW_HTTP"] {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_strata"));
        cmd.args(["api", "--listen", "127.0.0.1:0"]);
        for (k, v) in base {
            if k == missing {
                cmd.env_remove(k);
            } else {
                cmd.env(k, v);
            }
        }
        let out = cmd.output().unwrap();
        assert!(!out.status.success(), "{missing}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(missing),
            "{missing}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// Scenario "A setting that is missing" of the spec job-queue, and the role of the worker:
/// `strata worker` stops with an error that names the variable. Each case fails before the first
/// connection, so the test needs no service.
#[test]
fn worker_stops_when_a_setting_is_wrong() {
    let not_a_cgroup = std::env::temp_dir();
    let not_a_cgroup = not_a_cgroup.to_str().unwrap();
    let base = [
        (
            "STRATA_DATABASE_URL",
            "postgres://strata_worker@localhost:6432/strata",
        ),
        ("AWS_ENDPOINT", "http://localhost:3900"),
        ("AWS_ACCESS_KEY_ID", "key"),
        ("AWS_SECRET_ACCESS_KEY", "secret"),
        ("AWS_DEFAULT_REGION", "garage"),
        ("AWS_BUCKET_NAME", "strata"),
        ("AWS_ALLOW_HTTP", "true"),
        ("STRATA_NSJAIL", "/bin/sh"),
        ("STRATA_EXTRACT", "/bin/sh"),
        ("STRATA_CGROUP", not_a_cgroup),
    ];
    // (the variable, its value: `None` removes it)
    let cases = [
        ("STRATA_NSJAIL", None),
        ("STRATA_EXTRACT", None),
        ("STRATA_CGROUP", None),
        ("STRATA_EXTRACT", Some("/no/such/strata-extract")),
        ("STRATA_CGROUP", Some(not_a_cgroup)),
        // The spec job-queue: `strata worker` connects as `strata_worker` only.
        (
            "STRATA_DATABASE_URL",
            Some("postgres://strata_api@localhost:6432/strata"),
        ),
    ];
    for (name, value) in cases {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_strata"));
        cmd.arg("worker");
        for (k, v) in base {
            if k != name {
                cmd.env(k, v);
            }
        }
        match value {
            Some(v) => cmd.env(name, v),
            None => cmd.env_remove(name),
        };
        let out = cmd.output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{name} {value:?}");
        assert!(stderr.contains(name), "{name} {value:?}: {stderr}");
    }
}
