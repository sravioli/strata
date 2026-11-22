//! The server of Strata: `strata api`, `strata admin` and `strata migrate` (tech design 0049).

pub mod admin;
pub mod api;
pub mod auth;
pub mod db;
pub mod migrate;

/// The one source of the database URL of each subcommand.
pub const DATABASE_URL: &str = "STRATA_DATABASE_URL";

/// The database URL, or an error that names the variable.
pub fn database_url() -> Result<String, String> {
    std::env::var(DATABASE_URL)
        .map_err(|_| format!("set {DATABASE_URL} to the URL of the database"))
}
