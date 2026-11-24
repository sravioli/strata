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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    match run(Cli::parse()).await {
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
            let worker = strata::worker::Worker {
                pool: pool(cfg, POOL_SIZE, POOL_WAIT)?,
                storage: strata::storage::Storage::from_env()?,
                sandbox: strata::sandbox::Sandbox::from_env()?,
                settings: strata::worker::SETTINGS,
            };
            worker.run().await;
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
