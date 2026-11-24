//! `strata api`: the HTTP server. Each route takes a `Caller` (`crate::auth`) and decides with
//! `authorize` or `authorized_scope`.

use std::sync::Arc;

use axum::Router;
use axum::extract::FromRef;
use axum::routing::{get, post};

use crate::auth::{Auth, keys::KeyCache};
use crate::db::{self, POOL_SIZE, POOL_WAIT};
use crate::query::query;
use crate::status;
use crate::storage::Storage;
use crate::upload::{self, Bounds};

/// The state of the router.
#[derive(Clone)]
pub struct AppState {
    pub auth: Auth,
    pub storage: Arc<Storage>,
    pub bounds: Bounds,
    /// The `Retry-After` of a status that is not final, in seconds (0047).
    pub status_retry_after: u64,
}

impl FromRef<AppState> for Auth {
    fn from_ref(s: &AppState) -> Auth {
        s.auth.clone()
    }
}

/// The state of `strata api` on `pool`. The storage settings come from the environment, so a
/// missing variable stops the process before it serves a request.
pub fn state(pool: db::Pool) -> Result<AppState, String> {
    Ok(AppState {
        storage: Arc::new(Storage::from_env()?),
        auth: Auth::new(pool, Arc::new(KeyCache::new())),
        bounds: Bounds::default(),
        status_retry_after: crate::status::retry_after(
            std::env::var(crate::status::RETRY_AFTER_VAR)
                .ok()
                .as_deref(),
        )?,
    })
}

/// The router of the public API. A check reads each `.route(` call of this function
/// (TC-3-18), so each route has a test with the identifiers of a different tenant.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/documents", post(upload::start_document))
        .route(
            "/v1/documents/{document_id}/revisions",
            post(upload::start_revision),
        )
        .route(
            "/v1/documents/{document_id}/revisions/{revision}/finish",
            post(upload::finish),
        )
        .route(
            "/v1/documents/{document_id}/revisions/{revision}",
            get(status::one),
        )
        .route("/v1/revisions", query(status::batch))
        .with_state(state)
}

pub async fn serve(cfg: tokio_postgres::Config, listen: &str) -> Result<(), String> {
    let pool = db::pool(cfg, POOL_SIZE, POOL_WAIT)?;
    let app = router(state(pool.clone())?);
    // The cleanup of upload_slots has its own handle of the pool.
    tokio::spawn(crate::cleanup::old_slots_task(pool));
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|e| format!("bind {listen}: {e}"))?;
    tracing::info!(%listen, "strata api");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| e.to_string())
}
