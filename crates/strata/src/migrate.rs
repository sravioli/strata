//! `strata migrate`: the migrations of `migrations/`, embedded in the binary (tech design 0049).

mod embedded {
    refinery::embed_migrations!("migrations");
}

/// Applies each migration that the database does not have, as the role of the URL
/// (`strata_owner`). A migration whose text changed after it was applied stops the run.
pub async fn run(url: &str) -> Result<(), String> {
    let (mut client, conn) = tokio_postgres::connect(url, tokio_postgres::NoTls)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let task = tokio::spawn(conn);
    let report = embedded::migrations::runner()
        .run_async(&mut client)
        .await
        .map_err(|e| format!("migrate: {e}"))?;
    for m in report.applied_migrations() {
        println!("applied {m}");
    }
    drop(client);
    let _ = task.await;
    Ok(())
}
