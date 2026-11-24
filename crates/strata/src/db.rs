//! The pool, the identity transaction and the tenant transaction (tech designs 0003, 0004, 0049).
//!
//! Each function that queries a tenant-owned table takes `&mut TenantTx`. Only
//! `IdentityTx::into_tenant` makes one, and only from an "allow" decision or a scope.
//!
//! ```compile_fail
//! // A query function does not accept a pool.
//! async fn f(pool: &strata::db::Pool) {
//!     strata::db::tenant_context(pool, "x").await;
//! }
//! ```
//!
//! ```compile_fail
//! // A query function does not accept a plain connection.
//! async fn f(conn: &mut deadpool_postgres::Object) {
//!     strata::db::tenant_context(conn, "x").await;
//! }
//! ```
//!
//! TC-4-08: a route gets its tenant transaction only from a decision or a scope. The pool is
//! private to `auth` (see `strata::auth::Auth`).
//!
//! ```compile_fail
//! // A tenant transaction comes only from `into_tenant`.
//! fn f(id: &strata::db::IdentityTx, t: strata::db::TenantTx) -> strata::db::TenantTx {
//!     strata::db::TenantTx { tenant_id: id.tenant_id(), ..t }
//! }
//! ```
//!
//! ```compile_fail
//! // Only `allowed` makes an "allow", and only from `Decision::Allow`.
//! fn f() -> strata::db::Allowed {
//!     strata::db::Allowed(())
//! }
//! ```
//!
//! ```compile_fail
//! // A query function does not accept the identity transaction.
//! async fn f(mut id: strata::db::IdentityTx) {
//!     strata::db::tenant_context(&mut id, "x").await;
//! }
//! ```

use std::time::Duration;
use tracing::Instrument;

use deadpool_postgres::{Manager, ManagerConfig, Object, RecyclingMethod, Runtime};
use strata_core::authorization::{Decision, Principal, Scope};
use tokio_postgres::NoTls;
use uuid::Uuid;

pub use deadpool_postgres::{Pool, PoolError};

pub mod job;
pub mod search;
pub mod status;
pub mod upload;

/// Connections in the pool of each process (0049, "Values").
pub const POOL_SIZE: usize = 20;
/// The wait for a connection. Then the API answers 503 (0049, "Values").
pub const POOL_WAIT: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no connection in the pool: {0}")]
    Pool(#[from] PoolError),
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
}

impl Error {
    /// True when the wait for a connection reached its limit.
    pub fn is_pool_timeout(&self) -> bool {
        matches!(self, Error::Pool(PoolError::Timeout(_)))
    }

    /// A fixed class for the log: the SQLSTATE, or the kind of the error. The text of a PostgreSQL
    /// error can hold a value of a client, for example in its detail (0133).
    pub fn class(&self) -> &str {
        match self {
            Error::Pool(PoolError::Timeout(_)) => "pool timeout",
            Error::Pool(_) => "pool",
            Error::Postgres(e) if e.is_closed() => "connection closed",
            Error::Postgres(e) => e.code().map_or("postgres", |c| c.code()),
        }
    }

    /// True when PostgreSQL stopped the statement at its `statement_timeout`.
    pub fn is_statement_timeout(&self) -> bool {
        matches!(self, Error::Postgres(e) if e.code() == Some(&tokio_postgres::error::SqlState::QUERY_CANCELED))
    }
}

/// A pool for one database.
// shortcut: no TLS to PostgreSQL, because the skeleton runs only locally and in CI (0049).
// Add TLS before the first deployment (M3, #61).
pub fn pool(cfg: tokio_postgres::Config, size: usize, wait: Duration) -> Result<Pool, String> {
    let mgr = Manager::from_config(
        cfg,
        NoTls,
        ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        },
    );
    Pool::builder(mgr)
        .max_size(size)
        .wait_timeout(Some(wait))
        .create_timeout(Some(wait))
        .runtime(Runtime::Tokio1)
        .build()
        .map_err(|e| e.to_string())
}

/// A connection in an open transaction. A drop without `commit` or `rollback` closes the
/// connection, so that no open transaction goes back to the pool.
struct Tx(Option<Object>);

impl Tx {
    fn conn(&self) -> &Object {
        self.0.as_ref().expect("an open transaction")
    }

    async fn end(mut self, sql: &str) -> Result<(), Error> {
        let conn = self.0.take().expect("an open transaction");
        conn.batch_execute(sql).await?;
        Ok(())
    }

    /// The transaction ended on the connection: it goes back to the pool.
    fn release(mut self) {
        drop(self.0.take());
    }
}

impl Drop for Tx {
    fn drop(&mut self) {
        if let Some(conn) = self.0.take() {
            drop(Object::take(conn));
        }
    }
}

/// "Allow" for the request. Only `allowed` makes one, and only from `Decision::Allow`.
pub struct Allowed(());

pub fn allowed(d: Decision) -> Option<Allowed> {
    matches!(d, Decision::Allow).then_some(Allowed(()))
}

/// What opens a tenant transaction: an "allow" decision or a scope (0004).
pub enum Grant {
    Allowed(Allowed),
    Scope(Scope),
}

impl From<Allowed> for Grant {
    fn from(a: Allowed) -> Self {
        Grant::Allowed(a)
    }
}

impl From<Scope> for Grant {
    fn from(s: Scope) -> Self {
        Grant::Scope(s)
    }
}

type Params<'a> = &'a [&'a (dyn tokio_postgres::types::ToSql + Sync)];

/// The result of the identity steps 7 to 10 of 0021.
pub enum Identity {
    /// The binding exists, and the tenant and the account are active.
    Active(Box<IdentityTx>),
    /// No binding for the issuer and the tenant value.
    NoBinding,
    /// The tenant or the account is not active.
    NotActive,
}

/// The identity transaction of 0004. It can only read the binding, read or create the account,
/// and read the statuses. Its tenant context is the tenant of the binding.
pub struct IdentityTx {
    tx: Tx,
    tenant_id: Uuid,
    principal: Principal,
}

const BIND_CONTEXT: &str =
    "select set_config('app.issuer_id', $1::text, true), set_config('app.tenant_value', $2, true)";
// Step 8: the tenant context comes from the binding, in the statement that reads it.
const READ_BINDING: &str = "select tenant_id, set_config('app.tenant_id', tenant_id::text, true) \
     from issuer_bindings where issuer_id = $1 and tenant_value = $2";
const READ_TENANT: &str = "select status = 'active' from tenants \
     where tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid";
const READ_ACCOUNT: &str = "select account_id, status = 'active' from accounts \
     where tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid \
     and issuer_id = $1 and subject = $2";
const CREATE_ACCOUNT: &str = "insert into accounts (tenant_id, issuer_id, subject) \
     values (nullif(current_setting('app.tenant_id', true), '')::uuid, $1, $2) \
     on conflict do nothing";

impl IdentityTx {
    /// Steps 7 to 10 of 0021 in one transaction on the primary. The statements that need no
    /// earlier result go together (the pipelining rule of 0049).
    pub async fn open(
        pool: &Pool,
        issuer_id: Uuid,
        tenant_value: &str,
        subject: &str,
    ) -> Result<Identity, Error> {
        let tx = Tx(Some(pool.get().await?));
        let c = tx.conn();
        let issuer_text = issuer_id.to_string();
        let (bind, binding, tenant, account) = tokio::try_join!(
            c.prepare_cached(BIND_CONTEXT),
            c.prepare_cached(READ_BINDING),
            c.prepare_cached(READ_TENANT),
            c.prepare_cached(READ_ACCOUNT),
        )?;
        let (p_bind, p_binding, p_account): (Params, Params, Params) = (
            &[&issuer_text, &tenant_value],
            &[&issuer_id, &tenant_value],
            &[&issuer_id, &subject],
        );
        let (_, _, binding, tenant_active, account) = tokio::try_join!(
            c.batch_execute("begin"),
            c.query_one(&bind, p_bind)
                .instrument(crate::db::span("bind_context")),
            c.query_opt(&binding, p_binding)
                .instrument(crate::db::span("read_binding")),
            c.query_opt(&tenant, &[])
                .instrument(crate::db::span("read_tenant")),
            c.query_opt(&account, p_account)
                .instrument(crate::db::span("read_account")),
        )?;
        let Some(binding) = binding else {
            tx.end("rollback").await?;
            return Ok(Identity::NoBinding);
        };
        let tenant_id: Uuid = binding.get(0);
        let tenant_active = tenant_active.is_some_and(|r| r.get::<_, bool>(0));
        let account = match account {
            Some(row) => row,
            None => {
                // Step 9: a first request creates the account. Two at the same time give one.
                let create = c.prepare_cached(CREATE_ACCOUNT).await?;
                let read = c.prepare_cached(READ_ACCOUNT).await?;
                let (_, row) = tokio::try_join!(
                    c.execute(&create, p_account)
                        .instrument(crate::db::span("create_account")),
                    c.query_one(&read, p_account)
                        .instrument(crate::db::span("read_account"))
                )?;
                row
            }
        };
        let account_id: Uuid = account.get(0);
        let account_active: bool = account.get(1);
        if !(tenant_active && account_active) {
            tx.end("rollback").await?;
            return Ok(Identity::NotActive);
        }
        Ok(Identity::Active(Box::new(IdentityTx {
            tx,
            tenant_id,
            principal: Principal::Account {
                tenant: tenant_id.as_u128(),
                account: account_id.as_u128(),
                account_active,
                tenant_active,
            },
        })))
    }

    pub fn principal(&self) -> Principal {
        self.principal
    }

    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    /// The one way to open a tenant transaction. A scope "Nothing", or a scope of a different
    /// tenant, gives `None` and rolls the transaction back. The tenant context of the identity
    /// transaction stays: `app.tenant_id` was set with `set_config(…, true)` at step 8.
    pub async fn into_tenant(self, grant: impl Into<Grant>) -> Result<Option<TenantTx>, Error> {
        let ok = match grant.into() {
            Grant::Allowed(Allowed(())) => true,
            Grant::Scope(s) => s.tenant() == Some(self.tenant_id.as_u128()),
        };
        if !ok {
            self.tx.end("rollback").await?;
            return Ok(None);
        }
        Ok(Some(TenantTx {
            tx: self.tx,
            tenant_id: self.tenant_id,
        }))
    }
}

/// A transaction in the tenant context of one tenant.
pub struct TenantTx {
    tx: Tx,
    tenant_id: Uuid,
}

impl TenantTx {
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    pub async fn commit(self) -> Result<(), Error> {
        self.tx.end("commit").await
    }

    pub async fn rollback(self) -> Result<(), Error> {
        self.tx.end("rollback").await
    }

    /// Runs one statement and ends the transaction with `commit` or `rollback`, in one round trip
    /// (the pipelining rule of 0049). A failed statement aborts the transaction, so `commit`
    /// rolls it back, and the connection closes.
    async fn query_and_end(
        self,
        sql: &str,
        name: &'static str,
        params: Params<'_>,
        commit: bool,
    ) -> Result<Vec<tokio_postgres::Row>, Error> {
        let rows = {
            let c = self.tx.conn();
            let st = c.prepare_cached(sql).await?;
            let end = if commit { "commit" } else { "rollback" };
            let (rows, ()) = tokio::try_join!(
                c.query(&st, params).instrument(span(name)),
                c.batch_execute(end)
            )?;
            rows
        };
        self.tx.release();
        Ok(rows)
    }

    /// The connection of the transaction, for the L1 tests only.
    #[cfg(feature = "testing")]
    pub fn client(&self) -> &Object {
        self.tx.conn()
    }
}

/// The span of one query (0133): the name of its constant in lower case, and no SQL text and no
/// parameter.
/// A query outside a request or a job, as the poll of the worker, gets no span: one trace each
/// second is noise.
pub fn span(name: &'static str) -> tracing::Span {
    if tracing::Span::current().is_none() {
        return tracing::Span::none();
    }
    tracing::info_span!(
        "query",
        otel.name = name,
        otel.kind = "client",
        db.system.name = "postgresql",
    )
}

/// The tenant of the tenant context and one text that went to PostgreSQL as a parameter.
/// The L1 tests use it to show that the text of a request stays data (TC-3-16).
pub async fn tenant_context(tx: &mut TenantTx, text: &str) -> Result<(Uuid, String), Error> {
    let c = tx.tx.conn();
    let st = c
        .prepare_cached("select nullif(current_setting('app.tenant_id', true), '')::uuid, $1::text")
        .await?;
    let row = c.query_one(&st, &[&text]).await?;
    Ok((row.get(0), row.get(1)))
}

/// The test helper of 0003: a tenant transaction for `tenant`, through `into_tenant`.
#[cfg(feature = "testing")]
pub mod testing {
    use super::*;
    use strata_core::authorization::{Action, Resource, authorize};

    pub async fn tenant_tx(pool: &Pool, tenant: Uuid) -> Result<TenantTx, Error> {
        let tx = Tx(Some(pool.get().await?));
        let c = tx.conn();
        let set = c
            .prepare_cached("select set_config('app.tenant_id', $1::text, true)")
            .await?;
        let tenant_text = tenant.to_string();
        let p: Params = &[&tenant_text];
        tokio::try_join!(c.batch_execute("begin"), c.query_one(&set, p))?;
        let principal = Principal::Job {
            tenant: tenant.as_u128(),
            tenant_active: true,
        };
        let id = IdentityTx {
            tx,
            tenant_id: tenant,
            principal,
        };
        let d = authorize(
            principal,
            Action::ProcessDocument,
            Resource {
                tenant: tenant.as_u128(),
            },
        );
        let a = allowed(d).expect("a job of an active tenant is allowed on its own tenant");
        Ok(id
            .into_tenant(a)
            .await?
            .expect("an allow decision opens the transaction"))
    }
}
