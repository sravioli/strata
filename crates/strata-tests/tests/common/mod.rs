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

/// A pool of `strata_worker` through PgBouncer, as `strata worker` has it.
pub fn worker_pool() -> Pool {
    migrated();
    db::pool(
        config("strata_worker", PGBOUNCER),
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

/// A new database with the migrations, for a test that counts the rows of a global table. Other
/// tests write those tables at the same time in the shared database.
pub async fn new_database(su: &Client) -> String {
    migrated();
    let name = unique("strata_new").replace('-', "_");
    su.batch_execute(&format!("create database {name} owner strata_owner"))
        .await
        .unwrap();
    let mut owner = config("strata_owner", POSTGRES);
    owner.dbname(&name);
    strata::migrate::run(&owner).await.unwrap();
    name
}

/// The settings of `role` in the database `db`, directly on PostgreSQL.
pub fn config_in(role: &str, db: &str) -> Config {
    let mut c = config(role, POSTGRES);
    c.dbname(db);
    c
}

/// A new document of a tenant with one revision in `state`, as the superuser. Returns the
/// `document_id` and the `slot_id` of the revision.
pub async fn document(su: &Client, tenant_id: Uuid, state: &str) -> (Uuid, Uuid) {
    let error_class = (state == "Failed").then_some("MalformedFile");
    let row = su
        .query_one(
            "with d as (insert into documents (tenant_id, last_revision) values ($1, 1) \
             returning document_id) \
             insert into revisions (tenant_id, document_id, revision, state, file_name, \
             content_type, size, error_class, slot_id) \
             select $1, document_id, 1, $2, 'a.pdf', 'application/pdf', 10, $3, gen_random_uuid() \
             from d returning document_id, slot_id",
            &[&tenant_id, &state, &error_class],
        )
        .await
        .unwrap();
    (row.get(0), row.get(1))
}

/// The storage client with the development values of compose.yaml.
pub fn storage() -> strata::storage::Storage {
    storage_at("http://localhost:3900")
}

/// The storage client of `storage`, with a different endpoint.
pub fn storage_at(endpoint: &str) -> strata::storage::Storage {
    use strata::storage::*;
    let vars = [
        (ENDPOINT, endpoint),
        (ACCESS_KEY, "GK000000000000000000000000"),
        (
            SECRET_KEY,
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
        (REGION, "garage"),
        (BUCKET, "strata"),
        (ALLOW_HTTP, "true"),
    ];
    Storage::from_vars(|name| {
        vars.iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.to_string())
    })
    .unwrap()
}

/// A PUT of `body` to a signed URL over plain HTTP/1.1. The first `first` bytes go at once; the
/// rest go after `pause`. Returns the status code.
pub async fn put(url: &url::Url, body: &[u8], first: usize, pause: std::time::Duration) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let host = format!(
        "{}:{}",
        url.host_str().unwrap(),
        url.port_or_known_default().unwrap()
    );
    let mut s = tokio::net::TcpStream::connect(&host).await.unwrap();
    let target = match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().to_owned(),
    };
    let head = format!(
        "PUT {target} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes()).await.unwrap();
    s.write_all(&body[..first]).await.unwrap();
    s.flush().await.unwrap();
    tokio::time::sleep(pause).await;
    // The storage can answer and close before the rest arrives: the status is the result.
    let _ = s.write_all(&body[first..]).await;
    let mut resp = Vec::new();
    let _ = s.read_to_end(&mut resp).await;
    let line = String::from_utf8_lossy(&resp);
    line.split(' ')
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0)
}

/// The test program of the spec extraction-sandbox, a binary of this crate.
pub fn hostile() -> std::path::PathBuf {
    env!("CARGO_BIN_EXE_hostile-extract").into()
}

/// The `strata-extract` binary: `STRATA_EXTRACT`, or the binary of the build of this test.
pub fn strata_extract() -> std::path::PathBuf {
    if let Some(p) = std::env::var_os(strata::sandbox::EXTRACT) {
        return p.into();
    }
    // The profile folder of the build (`target/debug`, or the folder of `cargo llvm-cov`) holds
    // the binaries, and the test binary is below it.
    let exe = std::env::current_exe().unwrap();
    exe.ancestors()
        .map(|a| a.join("strata-extract"))
        .find(|p| p.is_file())
        .unwrap_or_else(|| {
            panic!(
                "build strata-extract first, or set STRATA_EXTRACT ({})",
                exe.display()
            )
        })
}

/// The sandbox of the worker around `extract`, in `cgroup` (default `STRATA_CGROUP`). The tests
/// run in the delegated cgroup of `scripts/in-cgroup.sh` (`make l1`): they never skip.
pub fn sandbox_in(
    extract: &std::path::Path,
    cgroup: Option<&std::path::Path>,
    limits: strata::sandbox::Limits,
) -> strata::sandbox::Sandbox {
    use strata::sandbox::*;
    let nsjail = std::env::var(NSJAIL)
        .unwrap_or_else(|_| format!("{}/../../.tools/bin/nsjail", env!("CARGO_MANIFEST_DIR")));
    let cgroup = match cgroup {
        Some(c) => c.display().to_string(),
        None => std::env::var(CGROUP)
            .expect("STRATA_CGROUP: run the L1 tests with make l1 (scripts/in-cgroup.sh)"),
    };
    let extract = extract.display().to_string();
    Sandbox::from_vars(
        |k| match k {
            NSJAIL => Some(nsjail.clone()),
            EXTRACT => Some(extract.clone()),
            CGROUP => Some(cgroup.clone()),
            _ => None,
        },
        limits,
    )
    .unwrap()
}

pub fn sandbox(
    extract: &std::path::Path,
    limits: strata::sandbox::Limits,
) -> strata::sandbox::Sandbox {
    sandbox_in(extract, None, limits)
}
