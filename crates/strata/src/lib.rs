//! The server of Strata: `strata api`, `strata admin` and `strata migrate` (tech design 0049).

pub mod admin;
pub mod api;
pub mod auth;
pub mod cleanup;
pub mod db;
pub mod extraction;
pub mod limits;
pub mod migrate;
pub mod query;
pub mod sandbox;
pub mod search;
pub mod status;
pub mod storage;
pub mod telemetry;
pub mod upload;
pub mod worker;

/// The one source of the database URL of each subcommand.
pub const DATABASE_URL: &str = "STRATA_DATABASE_URL";

/// The database settings of `STRATA_DATABASE_URL`, or an error that names the variable.
pub fn database_config() -> Result<tokio_postgres::Config, String> {
    let url = std::env::var(DATABASE_URL)
        .map_err(|_| format!("set {DATABASE_URL} to the URL of the database"))?;
    url.parse().map_err(|e| format!("{DATABASE_URL}: {e}"))
}

/// SIGINT or SIGTERM: the clean stop of `strata api` and `strata worker`.
pub async fn stop_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    match signal(SignalKind::terminate()) {
        Ok(mut term) => tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        },
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}
