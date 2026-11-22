//! `strata api`: the HTTP server. Change 1 of 0049 adds only the authentication of a request.
//! The routes come with the later changes. Each route takes a `Caller` (`crate::auth`).

use std::sync::Arc;

use axum::Router;
use axum::extract::FromRef;

use crate::auth::{Auth, keys::KeyCache};
use crate::db::{self, POOL_SIZE, POOL_WAIT};
use crate::storage::Storage;

/// The state of the router.
#[derive(Clone)]
pub struct AppState {
    pub auth: Auth,
    pub storage: Arc<Storage>,
}

impl FromRef<AppState> for Auth {
    fn from_ref(s: &AppState) -> Auth {
        s.auth.clone()
    }
}

/// The state of `strata api`. The storage settings come from the environment, so a missing
/// variable stops the process before it serves a request.
pub fn state(cfg: tokio_postgres::Config) -> Result<AppState, String> {
    let storage = Arc::new(Storage::from_env()?);
    let pool = db::pool(cfg, POOL_SIZE, POOL_WAIT)?;
    Ok(AppState {
        auth: Auth {
            pool,
            keys: Arc::new(KeyCache::new()),
        },
        storage,
    })
}

/// The router of the public API. It has no route yet.
pub fn router(state: AppState) -> Router {
    Router::new().with_state(state)
}

pub async fn serve(cfg: tokio_postgres::Config, listen: &str) -> Result<(), String> {
    let app = router(state(cfg)?);
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
