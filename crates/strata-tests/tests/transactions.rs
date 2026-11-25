//! L1 tests of the tenant transaction and the pooler (spec tenant-isolation).

mod common;

use common::*;
use strata::db::{self, Identity, IdentityTx, testing::tenant_tx};
use strata_core::authorization::{Action, authorized_scope};
use uuid::Uuid;

/// A pool of `strata_api` on the PgBouncer database of one server connection.
fn one_connection_pool() -> db::Pool {
    migrated();
    let mut cfg = config("strata_api", PGBOUNCER);
    cfg.dbname("strata_one");
    db::pool(cfg, 2, db::POOL_WAIT).unwrap()
}

/// TLS to PostgreSQL: the `sslmode` of the URL decides. The local services have no TLS, so
/// `require` gives no connection, and the default (`prefer`) connects without TLS.
#[tokio::test]
async fn sslmode_require_needs_tls() {
    migrated();
    let mut cfg = config("strata_api", POSTGRES);
    cfg.ssl_mode(tokio_postgres::config::SslMode::Require);
    let pool = db::pool(cfg, 1, db::POOL_WAIT).unwrap();
    assert!(pool.get().await.is_err(), "no TLS, no connection");
    let pool = db::pool(config("strata_api", POSTGRES), 1, db::POOL_WAIT).unwrap();
    assert!(pool.get().await.is_ok());
}

async fn tenants_seen(tx: &db::TenantTx) -> Vec<Uuid> {
    let rows = tx
        .client()
        .query("select tenant_id from accounts", &[])
        .await
        .unwrap();
    rows.iter().map(|r| r.get(0)).collect()
}

/// TC-3-05
#[tokio::test]
async fn next_transaction_on_a_pooled_connection_reads_only_its_own_tenant() {
    migrated();
    let su = connect(&superuser()).await;
    let (a, b) = (tenant(&su).await, tenant(&su).await);
    let (issuer, _) = issuer(&su, "https://p.example/jwks").await;
    account(&su, a, issuer).await;
    account(&su, b, issuer).await;
    let pool = one_connection_pool();
    let pid = "select pg_backend_pid()";

    let tx = tenant_tx(&pool, a).await.unwrap();
    let pid_a: i32 = tx.client().query_one(pid, &[]).await.unwrap().get(0);
    assert_eq!(tenants_seen(&tx).await, [a]);
    tx.commit().await.unwrap();

    let tx = tenant_tx(&pool, b).await.unwrap();
    let pid_b: i32 = tx.client().query_one(pid, &[]).await.unwrap().get(0);
    assert_eq!(pid_a, pid_b, "one server connection");
    assert_eq!(tenants_seen(&tx).await, [b]);
    tx.commit().await.unwrap();

    // Outside a transaction the setting is empty: the context of B did not stay.
    let c = pool.get().await.unwrap();
    let left: String = c
        .query_one(
            "select coalesce(current_setting('app.tenant_id', true), '')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(left, "");
}

#[tokio::test]
async fn prepared_statements_work_through_the_pooler() {
    migrated();
    let mut cfg = config("strata_api", PGBOUNCER);
    cfg.dbname("strata_one");
    let (c1, c2) = (connect(&cfg).await, connect(&cfg).await);
    let p = c1.prepare("select $1::int + 1").await.unwrap();
    let q = c2.prepare("select $1::text || '!'").await.unwrap();
    for _ in 0..3 {
        c1.batch_execute("begin").await.unwrap();
        assert_eq!(
            c1.query_one(&p, &[&1i32]).await.unwrap().get::<_, i32>(0),
            2
        );
        c1.batch_execute("commit").await.unwrap();
        c2.batch_execute("begin").await.unwrap();
        assert_eq!(
            c2.query_one(&q, &[&"a"]).await.unwrap().get::<_, String>(0),
            "a!"
        );
        c2.batch_execute("commit").await.unwrap();
    }
}

#[tokio::test]
async fn a_scope_of_nothing_opens_no_tenant_transaction() {
    migrated();
    let su = connect(&superuser()).await;
    let a = tenant(&su).await;
    let (issuer, _) = issuer(&su, "https://p.example/jwks").await;
    bind(&su, issuer, "acme", a).await;
    let pool = one_connection_pool();
    let Identity::Active(id) = IdentityTx::open(&pool, issuer, "acme", "u-1")
        .await
        .unwrap()
    else {
        panic!("an active identity")
    };
    // An account never gets a job action, so its scope is "Nothing".
    let scope = authorized_scope(id.principal(), Action::ProcessDocument);
    assert!(id.into_tenant(scope).await.unwrap().is_none());
    // The connection went back with no open transaction.
    let c = pool.get().await.unwrap();
    // Simple protocol: one message, so both times are equal only at the start of a transaction.
    let rows = c
        .simple_query("select now() = statement_timestamp()")
        .await
        .unwrap();
    let first = rows.iter().find_map(|m| match m {
        tokio_postgres::SimpleQueryMessage::Row(r) => r.get(0).map(str::to_owned),
        _ => None,
    });
    assert_eq!(first.as_deref(), Some("t"), "no open transaction");
}
