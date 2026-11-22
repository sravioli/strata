//! L1 tests of `strata admin` (spec operator-command). The commands run as `strata_operator`.

mod common;

use clap::Parser;
use common::*;
use strata::admin::{Admin, run};
use tokio_postgres::Client;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    admin: Admin,
}

async fn admin(c: &Client, args: &[&str]) -> Result<(), String> {
    let cmd = Cli::try_parse_from(std::iter::once("admin").chain(args.iter().copied()))
        .map_err(|e| e.to_string())?
        .admin;
    run(c, cmd).await
}

async fn operator() -> Client {
    migrated();
    connect(&config("strata_operator", POSTGRES)).await
}

async fn count(c: &Client, sql: &str, p: &str) -> i64 {
    c.query_one(sql, &[&p]).await.unwrap().get(0)
}

#[tokio::test]
async fn the_commands_need_the_operator_role() {
    migrated();
    let api = connect(&config("strata_api", POSTGRES)).await;
    assert!(
        admin(&api, &["tenant", "create", &unique("t")])
            .await
            .is_err()
    );
    let op = operator().await;
    let who: String = op
        .query_one("select current_user::text", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(who, "strata_operator");
    assert!(admin(&op, &["tenant", "list"]).await.is_ok());
}

#[tokio::test]
async fn tenants_create_name_rules_and_status() {
    let op = operator().await;
    let name = unique("acme");
    admin(&op, &["tenant", "create", &name]).await.unwrap();
    let status = "select status from tenants where name = $1";
    assert_eq!(
        op.query_one(status, &[&name])
            .await
            .unwrap()
            .get::<_, String>(0),
        "active"
    );
    admin(&op, &["tenant", "set-status", &name, "not-active"])
        .await
        .unwrap();
    assert_eq!(
        op.query_one(status, &[&name])
            .await
            .unwrap()
            .get::<_, String>(0),
        "not-active"
    );

    // Two tenants with one name.
    assert!(admin(&op, &["tenant", "create", &name]).await.is_err());
    assert_eq!(
        count(&op, "select count(*) from tenants where name = $1", &name).await,
        1
    );

    let long = format!("a{}", "b".repeat(63));
    for bad in ["Acme", "1acme", "acme_corp", "", long.as_str()] {
        assert!(
            admin(&op, &["tenant", "create", bad]).await.is_err(),
            "{bad}"
        );
        assert_eq!(
            count(&op, "select count(*) from tenants where name = $1", bad).await,
            0,
            "{bad}"
        );
    }
    assert!(
        admin(&op, &["tenant", "set-status", &unique("none"), "active"])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn issuers_register_with_defaults_and_reject_bad_values() {
    let op = operator().await;
    let iss = unique("https://p.example/i");
    let base = |extra: &[&'static str]| {
        let mut v = vec![
            "issuer",
            "register",
            "--issuer",
            "",
            "--jwks-url",
            "https://p.example/jwks",
            "--audience",
            "strata",
            "--tenant-claim",
            "org",
        ];
        v.extend_from_slice(extra);
        v
    };
    let mut args = base(&["--algorithm", "ES256"]);
    args[3] = &iss;
    admin(&op, &args).await.unwrap();
    let r = op
        .query_one(
            "select account_claim, max_lifetime_s, status from issuers where issuer = $1",
            &[&iss],
        )
        .await
        .unwrap();
    assert_eq!(
        (
            r.get::<_, String>(0),
            r.get::<_, i32>(1),
            r.get::<_, String>(2)
        ),
        ("sub".into(), 3600, "active".into())
    );

    for extra in [
        &["--algorithm", "ES256", "--max-lifetime", "0"][..],
        &["--algorithm", "ES256", "--max-lifetime", "3601"],
        &["--algorithm", "HS256"],
    ] {
        let other = unique("https://p.example/bad");
        let mut args = base(extra);
        args[3] = &other;
        assert!(admin(&op, &args).await.is_err(), "{extra:?}");
        assert_eq!(
            count(
                &op,
                "select count(*) from issuers where issuer = $1",
                &other
            )
            .await,
            0
        );
    }
    // A JWKS URL with HTTP to a host that is not loopback.
    let other = unique("https://p.example/http");
    let mut args = base(&["--algorithm", "EdDSA"]);
    args[3] = &other;
    args[5] = "http://p.example/jwks";
    assert!(admin(&op, &args).await.is_err());

    admin(
        &op,
        &[
            "issuer",
            "set",
            &iss,
            "--status",
            "not-active",
            "--max-lifetime",
            "60",
        ],
    )
    .await
    .unwrap();
    let r = op
        .query_one(
            "select max_lifetime_s, status from issuers where issuer = $1",
            &[&iss],
        )
        .await
        .unwrap();
    assert_eq!(
        (r.get::<_, i32>(0), r.get::<_, String>(1)),
        (60, "not-active".into())
    );
    assert!(
        admin(
            &op,
            &["issuer", "set", &unique("none"), "--status", "active"]
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn bindings_add_remove_and_one_tenant_for_each_value() {
    let op = operator().await;
    let su = connect(&superuser()).await;
    let (_, iss) = issuer(&su, "https://p.example/jwks").await;
    let (acme, globex) = (unique("acme"), unique("globex"));
    admin(&op, &["tenant", "create", &acme]).await.unwrap();
    admin(&op, &["tenant", "create", &globex]).await.unwrap();
    let bound = "select count(*) from issuer_bindings b join issuers i using (issuer_id) where i.issuer = $1";

    admin(
        &op,
        &[
            "binding",
            "add",
            "--issuer",
            &iss,
            "--tenant-value",
            "acme-corp",
            "--tenant",
            &acme,
        ],
    )
    .await
    .unwrap();
    assert_eq!(count(&op, bound, &iss).await, 1);
    let again = [
        "binding",
        "add",
        "--issuer",
        &iss,
        "--tenant-value",
        "acme-corp",
        "--tenant",
        &globex,
    ];
    assert!(admin(&op, &again).await.is_err());
    let tenant_of: String = op
        .query_one(
            "select t.name from issuer_bindings b join issuers i using (issuer_id) join tenants t using (tenant_id) \
             where i.issuer = $1",
            &[&iss],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(tenant_of, acme, "the first binding does not change");
    assert!(
        admin(
            &op,
            &[
                "binding",
                "add",
                "--issuer",
                &iss,
                "--tenant-value",
                "x",
                "--tenant",
                "nobody"
            ]
        )
        .await
        .is_err()
    );

    admin(
        &op,
        &[
            "binding",
            "remove",
            "--issuer",
            &iss,
            "--tenant-value",
            "acme-corp",
        ],
    )
    .await
    .unwrap();
    assert_eq!(count(&op, bound, &iss).await, 0);
    assert!(
        admin(
            &op,
            &[
                "binding",
                "remove",
                "--issuer",
                &iss,
                "--tenant-value",
                "acme-corp"
            ]
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn account_status() {
    let op = operator().await;
    let su = connect(&superuser()).await;
    let name = unique("acme");
    admin(&op, &["tenant", "create", &name]).await.unwrap();
    let (issuer_id, iss) = issuer(&su, "https://p.example/jwks").await;
    su.execute(
        "insert into accounts (tenant_id, issuer_id, subject) select tenant_id, $1, 'u-1' from tenants where name = $2",
        &[&issuer_id, &name],
    )
    .await
    .unwrap();
    let args = [
        "account",
        "set-status",
        "--tenant",
        &name,
        "--issuer",
        &iss,
        "--subject",
        "u-1",
        "not-active",
    ];
    admin(&op, &args).await.unwrap();
    let status: String = op
        .query_one(
            "select status from accounts where issuer_id = $1 and subject = 'u-1'",
            &[&issuer_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(status, "not-active");
    let none = [
        "account",
        "set-status",
        "--tenant",
        &name,
        "--issuer",
        &iss,
        "--subject",
        "nobody",
        "active",
    ];
    assert!(admin(&op, &none).await.is_err());
}
