//! `strata`: the server binary (tech design 0049). Each subcommand reads its database URL from
//! `STRATA_DATABASE_URL` only.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "strata", about = "Strata: tenant-scoped document search")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve the public API. Connect as strata_api, through the pooler.
    Api {
        #[arg(long, default_value = "127.0.0.1:8080")]
        listen: String,
    },
    /// Run the jobs. Connect as strata_worker, through the pooler.
    Worker,
    /// The operator command. Connect as strata_operator, directly.
    #[command(subcommand)]
    Admin(strata::admin::Admin),
    /// Apply the migrations of the binary. Connect as strata_owner, directly.
    Migrate,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    // Only the services export telemetry (tech design 0133).
    let service = match cli.cmd {
        Cmd::Api { .. } => Some("strata-api"),
        Cmd::Worker => Some("strata-worker"),
        _ => None,
    };
    let guard = match strata::telemetry::init(service.unwrap_or("strata"), service.is_some()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("error: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let result = run(cli).await;
    guard.shutdown().await;
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    let cfg = strata::database_config()?;
    match cli.cmd {
        Cmd::Api { listen } => strata::api::serve(cfg, &listen).await,
        Cmd::Migrate => strata::migrate::run(&cfg).await,
        Cmd::Worker => {
            use strata::db::{POOL_SIZE, POOL_WAIT, pool};
            // The spec job-queue: the worker connects as strata_worker, never with more rights.
            if cfg.get_user() != Some("strata_worker") {
                return Err("STRATA_DATABASE_URL: strata worker connects as strata_worker".into());
            }
            let worker = strata::worker::Worker {
                pool: pool(cfg, POOL_SIZE, POOL_WAIT)?,
                storage: strata::storage::Storage::from_env()?,
                sandbox: strata::sandbox::Sandbox::from_env()?,
                settings: strata::worker::SETTINGS,
                metrics: strata::telemetry::Metrics::global(),
            };
            tokio::select! {
                () = worker.run() => {}
                () = strata::stop_signal() => tracing::info!("strata worker stops"),
            }
            Ok(())
        }
        Cmd::Admin(cmd) => {
            let (client, conn) = cfg
                .connect(tokio_postgres::NoTls)
                .await
                .map_err(|e| format!("connect: {e}"))?;
            tokio::spawn(conn);
            strata::admin::run(&client, cmd).await
        }
    }
}
