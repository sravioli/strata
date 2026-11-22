//! `strata-dev`: the development issuer (tech design 0021). Laptop only.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use axum::{Json, Router, routing::get};
use clap::{Parser, Subcommand};
use strata_dev::{DEFAULT_LIFETIME_S, DevKey, register_commands};

#[derive(Parser)]
#[command(name = "strata-dev", about = "Development issuer of access tokens")]
struct Cli {
    /// The file of the Ed25519 key pair. The first run makes it.
    #[arg(long, default_value = ".strata-dev/ed25519.der")]
    key_file: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve the JWKS on 127.0.0.1, and print the commands that register the issuer.
    Serve {
        #[arg(long, default_value_t = 8090)]
        port: u16,
    },
    /// Print a token for a tenant value and a subject.
    Token {
        #[arg(long)]
        tenant: String,
        #[arg(long)]
        sub: String,
        /// The lifetime of the token, in seconds.
        #[arg(long, default_value_t = DEFAULT_LIFETIME_S)]
        lifetime: i64,
    },
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    let key = DevKey::load_or_create(&cli.key_file)?;
    match cli.cmd {
        Cmd::Token {
            tenant,
            sub,
            lifetime,
        } => println!("{}", key.token(&tenant, &sub, lifetime)),
        Cmd::Serve { port } => {
            let jwks = key.jwks();
            let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
            let listener = tokio::net::TcpListener::bind(addr).await?;
            print!(
                "{}",
                register_commands(&format!("http://{}/jwks.json", listener.local_addr()?))
            );
            let app = Router::new().route("/jwks.json", get(move || async move { Json(jwks) }));
            axum::serve(listener, app).await?;
        }
    }
    Ok(())
}
