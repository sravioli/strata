//! The cleanup of the global tables `jobs` (0005) and `upload_slots` (0006), with the values of
//! 0049: each 60 seconds, at most 1,000 rows in each batch. The tables hold no tenant content,
//! so the cleanup needs no tenant context.

use std::time::Duration;
use tracing::Instrument;

use tokio_postgres::Client;

use crate::db::Pool;

/// The rows that one run removes, at most (0049, "Values").
pub const BATCH: i64 = 1000;
/// The time between two runs (0049, "Values").
pub const EVERY: Duration = Duration::from_secs(60);

/// `Done` jobs that ended more than 24 hours ago (0005). Only `strata_worker` can remove rows of
/// `jobs`, so the worker of change 3 calls it.
const REMOVE_DONE_JOBS: &str = "delete from jobs where job_id in (select job_id from jobs \
     where state = 'Done' and ended_at < now() - interval '24 hours' order by ended_at limit $1)";

/// Rows of `upload_slots` older than the 2 hours of the bound (0006). A revision and its object do
/// not change: their cleanup waits for #69.
const REMOVE_OLD_SLOTS: &str = "delete from upload_slots where slot_id in (select slot_id \
     from upload_slots where created_at < now() - interval '2 hours' order by created_at limit $1)";

/// One batch of the cleanup of `Done` jobs, on a connection of `strata_worker`. Returns the count
/// of removed rows.
pub async fn remove_done_jobs(c: &Client) -> Result<u64, tokio_postgres::Error> {
    c.execute(REMOVE_DONE_JOBS, &[&BATCH])
        .instrument(crate::db::span("remove_done_jobs"))
        .await
}

/// One batch of the cleanup of `upload_slots`, on a connection of `strata_api`.
pub async fn remove_old_slots(c: &Client) -> Result<u64, tokio_postgres::Error> {
    c.execute(REMOVE_OLD_SLOTS, &[&BATCH])
        .instrument(crate::db::span("remove_old_slots"))
        .await
}

/// The task of `strata api` that runs `remove_old_slots` each 60 seconds. A failed run goes to the
/// log, and the next run continues.
pub async fn old_slots_task(pool: Pool) {
    let mut every = tokio::time::interval(EVERY);
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        every.tick().await;
        let run = async {
            let c = pool.get().await.map_err(crate::db::Error::from)?;
            remove_old_slots(&c).await.map_err(crate::db::Error::from)
        };
        match run.await {
            Ok(0) => {}
            Ok(n) => tracing::info!(rows = n, "removed old rows of upload_slots"),
            Err(e) => tracing::warn!(error = e.class(), "cleanup of upload_slots failed"),
        }
    }
}
