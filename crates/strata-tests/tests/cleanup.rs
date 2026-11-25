//! L1 tests of the cleanup of `jobs` and `upload_slots` (specs job-queue and document-upload).
//! Each counts the rows of a global table, so it runs in a new database.

mod common;

use common::*;
use strata::cleanup::{BATCH, remove_done_jobs, remove_old_slots};
use uuid::Uuid;

/// Jobs of one tenant: (state, hours since `ended_at` and `run_after`). Returns their ids.
async fn jobs(su: &tokio_postgres::Client, rows: &[(&str, i32)]) -> Vec<Uuid> {
    let t = tenant(su).await;
    let mut ids = Vec::new();
    for (state, hours) in rows {
        let ended = (*state == "Done").then_some(*hours);
        ids.push(
            su.query_one(
                "insert into jobs (tenant_id, kind, document_id, revision, state, run_after, ended_at) \
                 values ($1, 'ProcessDocument', uuidv7(), 1, $2, now() - make_interval(hours => $3), \
                 now() - make_interval(hours => $4)) returning job_id",
                &[&t, state, hours, &ended],
            )
            .await
            .unwrap()
            .get(0),
        );
    }
    ids
}

async fn left(su: &tokio_postgres::Client, ids: &[Uuid]) -> Vec<bool> {
    let mut out = Vec::new();
    for id in ids {
        out.push(
            su.query_one("select exists (select from jobs where job_id = $1)", &[id])
                .await
                .unwrap()
                .get(0),
        );
    }
    out
}

/// TC-5-10
#[tokio::test]
async fn a_job_of_23_hours_stays_and_a_job_of_25_hours_goes() {
    let db = new_database(&connect(&superuser()).await).await;
    let su = connect(&config_in("strata", &db)).await;
    let ids = jobs(&su, &[("Done", 23), ("Done", 25)]).await;
    let worker = connect(&config_in("strata_worker", &db)).await;
    assert_eq!(remove_done_jobs(&worker).await.unwrap(), 1);
    assert_eq!(left(&su, &ids).await, [true, false]);
}

#[tokio::test]
async fn jobs_in_other_states_stay() {
    let db = new_database(&connect(&superuser()).await).await;
    let su = connect(&config_in("strata", &db)).await;
    let ids = jobs(&su, &[("Waiting", 25), ("Running", 25), ("Parked", 25)]).await;
    let worker = connect(&config_in("strata_worker", &db)).await;
    assert_eq!(remove_done_jobs(&worker).await.unwrap(), 0);
    assert_eq!(left(&su, &ids).await, [true, true, true]);
}

#[tokio::test]
async fn one_run_removes_at_most_one_batch() {
    let db = new_database(&connect(&superuser()).await).await;
    let su = connect(&config_in("strata", &db)).await;
    let t = tenant(&su).await;
    su.execute(
        "insert into jobs (tenant_id, kind, document_id, revision, state, ended_at) \
         select $1, 'ProcessDocument', uuidv7(), 1, 'Done', now() - interval '25 hours' \
         from generate_series(1, $2::int)",
        &[&t, &(BATCH as i32 + 1)],
    )
    .await
    .unwrap();
    su.execute(
        "insert into upload_slots (created_at) select now() - interval '3 hours' \
         from generate_series(1, $1::int)",
        &[&(BATCH as i32 + 1)],
    )
    .await
    .unwrap();
    let worker = connect(&config_in("strata_worker", &db)).await;
    assert_eq!(remove_done_jobs(&worker).await.unwrap(), BATCH as u64);
    let api = connect(&config_in("strata_api", &db)).await;
    assert_eq!(remove_old_slots(&api).await.unwrap(), BATCH as u64);
}

/// "Old rows go and new rows stay": one run of the task of `strata api`.
#[tokio::test]
async fn old_rows_of_upload_slots_go_and_new_rows_stay() {
    let db = new_database(&connect(&superuser()).await).await;
    let su = connect(&config_in("strata", &db)).await;
    let t = tenant(&su).await;
    let (old_doc, old_slot) = document(&su, t, "AwaitingUpload").await;
    let (new_doc, new_slot) = document(&su, t, "AwaitingUpload").await;
    su.execute(
        "insert into upload_slots (slot_id, created_at) values \
         ($1, now() - interval '3 hours'), ($2, now() - interval '1 hour')",
        &[&old_slot, &new_slot],
    )
    .await
    .unwrap();
    let api = connect(&config_in("strata_api", &db)).await;
    // A second API process that runs the cleanup now: this run removes nothing.
    let mut other = connect(&config_in("strata_api", &db)).await;
    let held = other.transaction().await.unwrap();
    held.execute(
        "select pg_advisory_xact_lock(hashtext('strata.upload_slots'))",
        &[],
    )
    .await
    .unwrap();
    assert_eq!(remove_old_slots(&api).await.unwrap(), 0);
    held.rollback().await.unwrap();
    assert_eq!(remove_old_slots(&api).await.unwrap(), 1);
    let slots: Vec<Uuid> = su
        .query("select slot_id from upload_slots", &[])
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(slots, [new_slot]);
    // The revision of each row does not change.
    for d in [old_doc, new_doc] {
        let state: String = su
            .query_one("select state from revisions where document_id = $1", &[&d])
            .await
            .unwrap()
            .get(0);
        assert_eq!(state, "AwaitingUpload");
    }
}
