//! Helpers of the L1 tests: connections to the services of compose.yaml, the migrations, and
//! the rows that a test needs. Each test makes its own tenants and issuers with new names, so the
//! tests can run at the same time, and on a database that holds the data of earlier runs.
#![allow(dead_code)] // Each test file uses a part of the helpers.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use strata::db::{self, Pool};
use tokio_postgres::{Client, Config, NoTls};
use uuid::Uuid;

pub const POSTGRES: u16 = 5432;
pub const PGBOUNCER: u16 = 6432;

/// The settings of a role of dev/roles.sql. Each development password is the name of its role.
pub fn config(role: &str, port: u16) -> Config {
    let mut c = Config::new();
    c.host("localhost")
        .port(port)
        .user(role)
        .password(role)
        .dbname("strata");
    c
}

/// The superuser of compose.yaml, for the rows that a test needs.
pub fn superuser() -> Config {
    config("strata", POSTGRES)
}

pub async fn connect(cfg: &Config) -> Client {
    let (client, conn) = cfg.connect(NoTls).await.expect("a service of compose.yaml");
    tokio::spawn(conn);
    client
}

/// Applies the migrations one time for each test binary, as `strata_owner`.
pub fn migrated() {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        std::thread::spawn(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(strata::migrate::run(&config("strata_owner", POSTGRES)))
                .expect("migrations");
        })
        .join()
        .unwrap();
    });
}

/// A pool of `strata_api` through PgBouncer, as `strata api` has it.
pub fn api_pool() -> Pool {
    migrated();
    db::pool(
        config("strata_api", PGBOUNCER),
        db::POOL_SIZE,
        db::POOL_WAIT,
    )
    .unwrap()
}

/// A new name that no earlier run used: a valid tenant name.
pub fn unique(prefix: &str) -> String {
    static N: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "{prefix}-{nanos:x}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

/// A new active tenant.
pub async fn tenant(su: &Client) -> Uuid {
    su.query_one(
        "insert into tenants (name) values ($1) returning tenant_id",
        &[&unique("t")],
    )
    .await
    .unwrap()
    .get(0)
}

/// A new active issuer with EdDSA.
pub async fn issuer(su: &Client, jwks_url: &str) -> (Uuid, String) {
    issuer_with(su, jwks_url, "EdDSA").await
}

pub async fn issuer_with(su: &Client, jwks_url: &str, alg: &str) -> (Uuid, String) {
    let name = unique("https://issuer.example/p");
    let row = su
        .query_one(
            "insert into issuers (issuer, jwks_url, audience, algorithm, tenant_claim) \
             values ($1, $2, 'strata', $3, 'org') returning issuer_id",
            &[&name, &jwks_url, &alg],
        )
        .await
        .unwrap();
    (row.get(0), name)
}

pub async fn bind(su: &Client, issuer_id: Uuid, tenant_value: &str, tenant_id: Uuid) {
    su.execute(
        "insert into issuer_bindings (issuer_id, tenant_value, tenant_id) values ($1, $2, $3)",
        &[&issuer_id, &tenant_value, &tenant_id],
    )
    .await
    .unwrap();
}

/// A new active account of a tenant.
pub async fn account(su: &Client, tenant_id: Uuid, issuer_id: Uuid) -> Uuid {
    su.query_one(
        "insert into accounts (tenant_id, issuer_id, subject) values ($1, $2, $3) returning account_id",
        &[&tenant_id, &issuer_id, &unique("u")],
    )
    .await
    .unwrap()
    .get(0)
}

/// The SQLSTATE of a failed statement.
pub fn sqlstate<T: std::fmt::Debug>(r: Result<T, tokio_postgres::Error>) -> String {
    r.expect_err("the statement fails")
        .code()
        .map(|c| c.code().to_owned())
        .unwrap_or_default()
}
