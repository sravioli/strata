//! `strata api`: the HTTP server. Each route takes a `Caller` (`crate::auth`) and decides with
//! `authorize` or `authorized_scope`.

use std::sync::Arc;

use axum::Router;
use axum::extract::{FromRef, MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
use tracing::Instrument;
use tracing::field::Empty;

use crate::auth::{Auth, keys::KeyCache};
use crate::db::{self, POOL_SIZE, POOL_WAIT};
use crate::limits::{Buckets, Clock, MAX_BUCKETS, MAX_REQUESTS};
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
    /// The token buckets of the status routes (0047).
    pub buckets: Arc<Buckets>,
    /// The permits of the requests at the same time (`limits::MAX_REQUESTS`).
    pub in_flight: Arc<tokio::sync::Semaphore>,
    /// The statement timeout of a search (`search::TIMEOUT`).
    pub search_timeout: std::time::Duration,
    /// The instruments of 0133.
    pub metrics: Arc<crate::telemetry::Metrics>,
}

impl FromRef<AppState> for Auth {
    fn from_ref(s: &AppState) -> Auth {
        s.auth.clone()
    }
}

/// The state of `strata api` on `pool`. The storage settings come from the environment, so a
/// missing variable stops the process before it serves a request.
pub fn state(pool: db::Pool) -> Result<AppState, String> {
    let retry_after = std::env::var(crate::status::RETRY_AFTER_VAR).ok();
    let status_retry_after = crate::status::retry_after(retry_after.as_deref())?;
    Ok(AppState {
        storage: Arc::new(Storage::from_env()?),
        auth: Auth::new(pool, Arc::new(KeyCache::new())),
        bounds: Bounds::default(),
        status_retry_after,
        buckets: Arc::new(Buckets::new(
            status_retry_after,
            MAX_BUCKETS,
            Clock::system(),
        )),
        in_flight: Arc::new(tokio::sync::Semaphore::new(MAX_REQUESTS)),
        search_timeout: crate::search::TIMEOUT,
        metrics: crate::telemetry::Metrics::global(),
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
        .route("/v1/search", query(crate::search::search))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::limits::bound,
        ))
        .layer(axum::middleware::from_fn_with_state(state.clone(), trace))
        .with_state(state)
}

/// One span for each request, and its duration (0133). The span starts a new trace: the API
/// does not read a `traceparent` header of a client, an untrusted value.
async fn trace(State(s): State<AppState>, req: Request, next: Next) -> Response {
    let method = req.method().as_str().to_owned();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned());
    let name = match &route {
        Some(r) => format!("{method} {r}"),
        None => method.clone(),
    };
    let span = tracing::info_span!(
        "request",
        otel.name = %name,
        otel.kind = "server",
        http.request.method = %method,
        http.route = route.as_deref(),
        http.response.status_code = Empty,
        strata.tenant_id = Empty,
        strata.account_id = Empty,
        strata.search.q_length = Empty,
        strata.search.limit = Empty,
    );
    let start = std::time::Instant::now();
    let response = next.run(req).instrument(span.clone()).await;
    let status = response.status().as_u16();
    span.record("http.response.status_code", status);
    s.metrics.request(
        &method,
        route.as_deref(),
        status,
        start.elapsed().as_secs_f64(),
    );
    response
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
        .with_graceful_shutdown(crate::stop_signal())
        .await
        .map_err(|e| e.to_string())
}
