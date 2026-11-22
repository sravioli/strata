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
