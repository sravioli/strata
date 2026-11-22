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
