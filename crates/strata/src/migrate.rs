//! `strata migrate`: the migrations of `migrations/`, embedded in the binary (tech design 0049).

mod embedded {
    refinery::embed_migrations!("migrations");
}

/// The warning of `strata migrate` for a database that the setup step of 0049 did not change.
pub const LEAKPROOF_WARNING: &str = "warning: ts_match_vq(tsvector, tsquery) is not leakproof in this \
     database, so a search reads each text part of the tenant and no index. As a superuser, run: \
     alter function ts_match_vq(tsvector, tsquery) leakproof;";

/// True when the match of a search is `leakproof` in the database of `c` (0049, #43). Only a
/// superuser can set the mark, so `strata migrate` checks it and does not set it.
pub async fn leakproof_match(c: &tokio_postgres::Client) -> Result<bool, tokio_postgres::Error> {
    let row = c
        .query_one(
            "select proleakproof from pg_proc where oid = 'ts_match_vq(tsvector, tsquery)'::regprocedure",
            &[],
        )
        .await?;
    Ok(row.get(0))
}

/// Applies each migration that the database does not have, as the role of `cfg`
/// (`strata_owner`). A migration whose text changed after it was applied stops the run.
pub async fn run(cfg: &tokio_postgres::Config) -> Result<(), String> {
    let (mut client, conn) = cfg
        .connect(crate::db::tls()?)
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
    match leakproof_match(&client).await {
        Ok(true) => {}
        Ok(false) => eprintln!("{LEAKPROOF_WARNING}"),
        Err(e) => eprintln!("migrate: the check of ts_match_vq failed: {e}"),
    }
    drop(client);
    let _ = task.await;
    Ok(())
}
