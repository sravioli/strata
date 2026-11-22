//! L1 tests of the worker: the claim, the lease and the end of a job (specs job-queue,
//! revision-lifecycle, document-text and tenant-isolation). `jobs` is a global table, and a
//! claim takes any due job, so each test runs in a new database.

mod common;

use common::*;
use strata::db::job::{ClaimTx, renew};
use strata::db::{POOL_SIZE, POOL_WAIT, Pool, pool};
use strata_core::authorization::{Decision, DenyReason};
use strata_core::job::JobState;
use tokio_postgres::Client;
use uuid::Uuid;

/// A new database, its superuser client and a pool of `strata_worker` as `strata worker` has it.
struct Db {
    su: Client,
    worker: Pool,
}

async fn db() -> Db {
    let name = new_database(&connect(&superuser()).await).await;
    Db {
        su: connect(&config_in("strata", &name)).await,
        worker: pool(config_in("strata_worker", &name), POOL_SIZE, POOL_WAIT).unwrap(),
    }
}

/// An `Uploaded` revision 1 of a new document of `tenant`, and its job with the given columns.
async fn job(su: &Client, tenant: Uuid, set: &str) -> (Uuid, Uuid) {
    let (doc, _) = document(su, tenant, "Uploaded").await;
    let id = su
        .query_one(
            "insert into jobs (tenant_id, kind, document_id, revision, run_after) \
             values ($1, 'ProcessDocument', $2, 1, now() - interval '1 second') returning job_id",
            &[&tenant, &doc],
        )
        .await
        .unwrap()
        .get(0);
    if !set.is_empty() {
        su.execute(&format!("update jobs set {set} where job_id = $1"), &[&id])
            .await
            .unwrap();
    }
    (doc, id)
}

/// (state, attempts, last_error) of a job.
async fn row(su: &Client, id: Uuid) -> (String, i32, String) {
    let r = su
        .query_one(
            "select state, attempts, last_error from jobs where job_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    (r.get(0), r.get(1), r.get(2))
}

/// TC-5-02
#[tokio::test]
async fn two_workers_and_one_job() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (_, id) = job(&d.su, t, "").await;
    // The first claim holds the row lock until it commits: the second skips the job.
    let first = ClaimTx::claim(&d.worker)
        .await
        .unwrap()
        .expect("one job is due");
    assert!(ClaimTx::claim(&d.worker).await.unwrap().is_none());
    assert_eq!(first.claimed().job_id, id);
    assert_eq!(first.claimed().job.state, JobState::Running);
    let tx = first.into_tenant().await.unwrap().expect("allow");
    tx.commit().await.unwrap();
    // After the commit the lease protects the job.
    assert!(ClaimTx::claim(&d.worker).await.unwrap().is_none());
    assert_eq!(
        row(&d.su, id).await,
        ("Running".into(), 1, "NoError".into())
    );
}

/// Scenario "A job that is not due".
#[tokio::test]
async fn a_job_that_is_not_due() {
    let d = db().await;
    let t = tenant(&d.su).await;
    job(&d.su, t, "run_after = now() + interval '1 minute'").await;
    job(
        &d.su,
        t,
        "state = 'Running', attempts = 1, lease_until = now() + interval '1 minute', lease_token = gen_random_uuid()",
    )
    .await;
    assert!(ClaimTx::claim(&d.worker).await.unwrap().is_none());
}

/// Scenario "A RemoveFiles job".
#[tokio::test]
async fn a_remove_files_job() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (_, id) = job(&d.su, t, "kind = 'RemoveFiles'").await;
    assert!(ClaimTx::claim(&d.worker).await.unwrap().is_none());
    assert_eq!(
        row(&d.su, id).await,
        ("Waiting".into(), 0, "NoError".into())
    );
}

/// Scenario "An expired lease at the maximum of attempts".
#[tokio::test]
async fn an_expired_lease_at_the_maximum_of_attempts() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (_, id) = job(
        &d.su,
        t,
        "state = 'Running', attempts = 5, lease_until = now() - interval '1 second', lease_token = gen_random_uuid()",
    )
    .await;
    let claim = ClaimTx::claim(&d.worker)
        .await
        .unwrap()
        .expect("the job is due");
    assert!(claim.into_tenant().await.unwrap().is_none(), "no job");
    assert_eq!(
        row(&d.su, id).await,
        ("Parked".into(), 5, "LeaseExpired".into())
    );
}

/// An expired lease below the maximum: the job runs again, with a new token.
#[tokio::test]
async fn an_expired_lease_gives_the_job_to_a_second_worker() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let old = Uuid::from_u128(42);
    let (_, id) = job(
        &d.su,
        t,
        &format!("state = 'Running', attempts = 1, lease_until = now() - interval '1 second', lease_token = '{old}'"),
    )
    .await;
    let claim = ClaimTx::claim(&d.worker)
        .await
        .unwrap()
        .expect("the job is due");
    let token = claim.claimed().lease_token.expect("a new token");
    assert_ne!(token, old);
    claim
        .into_tenant()
        .await
        .unwrap()
        .unwrap()
        .commit()
        .await
        .unwrap();
    // `job_step` keeps the error of the expired lease in `last_error`.
    assert_eq!(
        row(&d.su, id).await,
        ("Running".into(), 2, "LeaseExpired".into())
    );
    // Scenario "A renewal after a new claim": the first worker renews with its old token.
    assert!(!renew(&d.worker, id, old).await.unwrap());
    assert!(renew(&d.worker, id, token).await.unwrap());
}

/// TC-4-10, and the first half of TC-5-05.
#[tokio::test]
async fn a_tenant_stops_while_a_job_runs() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (_, first) = job(&d.su, t, "").await;
    let claim = ClaimTx::claim(&d.worker).await.unwrap().unwrap();
    assert_eq!(claim.decision(), Decision::Allow);
    let mut running = claim.into_tenant().await.unwrap().expect("allow");
    let (_, second) = job(&d.su, t, "").await;
    d.su.execute(
        "update tenants set status = 'not-active' where tenant_id = $1",
        &[&t],
    )
    .await
    .unwrap();

    // The job that runs continues: its tenant transaction still reads and writes.
    let (tenant_id, _) = strata::db::tenant_context(&mut running, "x").await.unwrap();
    assert_eq!(tenant_id, t);
    running.commit().await.unwrap();

    // The next claim of a job of that tenant gets "deny", and the worker does not start it.
    let claim = ClaimTx::claim(&d.worker).await.unwrap().unwrap();
    assert_eq!(claim.claimed().job_id, second);
    assert_eq!(claim.decision(), Decision::Deny(DenyReason::NotActive));
    assert!(claim.into_tenant().await.unwrap().is_none());
    assert_eq!(
        row(&d.su, second).await,
        ("Parked".into(), 0, "TenantNotActive".into())
    );
    assert_eq!(row(&d.su, first).await.0, "Running");
}
