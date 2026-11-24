//! L1 tests of the worker: the claim, the lease and the end of a job (specs job-queue,
//! revision-lifecycle, document-text and tenant-isolation). `jobs` is a global table, and a
//! claim takes any due job, so each test runs in a new database.

mod common;

use common::*;
use std::time::Duration;
use strata::db::job::{ClaimTx, Claimed, End, Finish, end_job, keep_lease, renew};
use strata::db::{POOL_SIZE, POOL_WAIT, Pool, pool};
use strata::extraction::Part;
use strata::sandbox::{LIMITS, Limits, Stop};
use strata::worker::{Settings, Worker};
use strata_core::authorization::{Decision, DenyReason};
use strata_core::job::{JobState, MAX_ATTEMPTS};
use strata_core::revision::{Event as RevisionEvent, FailureClass};
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
    // The renewal that writes nothing stops the extraction of the first worker.
    let s = sandbox(
        &hostile(),
        Limits {
            wall: Duration::from_secs(60),
            ..LIMITS
        },
    );
    let every = Duration::from_millis(100);
    let run = s
        .run(b"forever", keep_lease(&d.worker, id, old, every))
        .await;
    assert_eq!(run.stop, Stop::Cancelled);
    // The lease of the second worker holds: its renewals continue.
    let kept = tokio::time::timeout(
        Duration::from_millis(500),
        keep_lease(&d.worker, id, token, every),
    );
    assert!(kept.await.is_err(), "the lease of the claim holds");
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

// The end of a job (task 5.4).

/// Revision `n` of a document, as the superuser, and its `Waiting` job. Returns the job.
async fn revision(su: &Client, tenant: Uuid, doc: Uuid, n: i32, state: &str) -> Uuid {
    su.execute(
        "with r as (insert into revisions (tenant_id, document_id, revision, state, file_name, \
         content_type, size, slot_id) \
         values ($1, $2, $3, $4, 'a.pdf', 'application/pdf', 10, gen_random_uuid())) \
         update documents set last_revision = $3 where document_id = $2",
        &[&tenant, &doc, &n, &state],
    )
    .await
    .unwrap();
    job_of(su, tenant, doc, n).await
}

/// A `Waiting` and due job of revision `n`.
async fn job_of(su: &Client, tenant: Uuid, doc: Uuid, n: i32) -> Uuid {
    su.query_one(
        "insert into jobs (tenant_id, kind, document_id, revision, run_after) \
         values ($1, 'ProcessDocument', $2, $3, now() - interval '1 second') returning job_id",
        &[&tenant, &doc, &n],
    )
    .await
    .unwrap()
    .get(0)
}

/// (state, retry_mark, error_class, no_text, status_at) of a revision.
type Rev = (String, bool, Option<String>, bool, std::time::SystemTime);

async fn rev(su: &Client, doc: Uuid, n: i32) -> Rev {
    let r = su
        .query_one(
            "select state, retry_mark, error_class, no_text, status_at from revisions \
             where document_id = $1 and revision = $2",
            &[&doc, &n],
        )
        .await
        .unwrap();
    (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4))
}

/// The text of the parts of a revision, in the order of `part`.
async fn parts(su: &Client, doc: Uuid, n: i32) -> Vec<String> {
    su.query(
        "select part_text from text_parts where document_id = $1 and revision = $2 order by part",
        &[&doc, &n],
    )
    .await
    .unwrap()
    .iter()
    .map(|r| r.get(0))
    .collect()
}

async fn ready_revision(su: &Client, doc: Uuid) -> Option<i32> {
    su.query_one(
        "select ready_revision from documents where document_id = $1",
        &[&doc],
    )
    .await
    .unwrap()
    .get(0)
}

/// Claims the one due job and commits the claim, as the worker does before the run.
async fn claim(d: &Db) -> Claimed {
    let c = ClaimTx::claim(&d.worker).await.unwrap().expect("a due job");
    let claimed = c.claimed().clone();
    c.into_tenant()
        .await
        .unwrap()
        .expect("allow")
        .commit()
        .await
        .unwrap();
    claimed
}

/// The transaction that ends a claimed job, as the worker opens it after the run.
async fn end(pool: &Pool, c: &Claimed, f: Finish) -> End {
    let tx = ClaimTx::resume(pool, c).await.unwrap();
    end_job(tx.into_tenant().await.unwrap().unwrap(), c, f)
        .await
        .unwrap()
}

fn text(words: &str) -> Finish {
    Finish::Extraction(
        RevisionEvent::Extracted(false),
        vec![Part {
            location: 1,
            text: words.into(),
        }],
    )
}

/// Scenario "The worker writes the result of the function", and "A Failed revision has no
/// text parts". A result that sets the retry mark ends the job with `Fail`, each other result
/// with `Succeed`.
#[tokio::test]
async fn the_worker_writes_the_result_of_the_function() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (doc, id) = job(&d.su, t, "").await;
    let c = claim(&d).await;
    let crash = Finish::Extraction(
        RevisionEvent::ExtractFailed(FailureClass::ExtractionCrash),
        Vec::new(),
    );
    assert!(matches!(end(&d.worker, &c, crash).await, End::Done(_)));
    let (state, mark, class, ..) = rev(&d.su, doc, 1).await;
    assert_eq!((state.as_str(), mark, class), ("Uploaded", true, None));
    assert_eq!(
        row(&d.su, id).await,
        ("Waiting".into(), 1, "JobFailed".into())
    );

    d.su.execute(
        "update jobs set run_after = now() where job_id = $1",
        &[&id],
    )
    .await
    .unwrap();
    let c = claim(&d).await;
    let malformed = Finish::Extraction(
        RevisionEvent::ExtractFailed(FailureClass::MalformedFile),
        Vec::new(),
    );
    end(&d.worker, &c, malformed).await;
    let (state, mark, class, ..) = rev(&d.su, doc, 1).await;
    assert_eq!(
        (state.as_str(), mark, class.as_deref()),
        ("Failed", true, Some("MalformedFile"))
    );
    assert!(parts(&d.su, doc, 1).await.is_empty());
    assert_eq!(row(&d.su, id).await.0, "Done");
}

/// Scenarios "A Ready revision becomes the ready revision of its document" and "A newer Ready
/// revision removes the text parts of the older one".
#[tokio::test]
async fn a_newer_ready_revision_supersedes_the_older_one() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (doc, _) = job(&d.su, t, "").await;
    let c = claim(&d).await;
    end(&d.worker, &c, text("one")).await;
    assert_eq!(rev(&d.su, doc, 1).await.0, "Ready");
    assert_eq!(parts(&d.su, doc, 1).await, ["one"]);
    assert_eq!(ready_revision(&d.su, doc).await, Some(1));

    let before = rev(&d.su, doc, 1).await.4;
    revision(&d.su, t, doc, 2, "Uploaded").await;
    let c = claim(&d).await;
    end(&d.worker, &c, text("two")).await;
    let old = rev(&d.su, doc, 1).await;
    assert_eq!(old.0, "Superseded");
    assert!(old.4 > before, "a new status_at");
    assert!(parts(&d.su, doc, 1).await.is_empty());
    assert_eq!(rev(&d.su, doc, 2).await.0, "Ready");
    assert_eq!(parts(&d.su, doc, 2).await, ["two"]);
    assert_eq!(ready_revision(&d.su, doc).await, Some(2));
}

/// A document with `Uploaded` revisions 1 and 2, and the claims of their two jobs.
async fn two_claims(d: &Db) -> (Uuid, Claimed, Claimed) {
    let t = tenant(&d.su).await;
    let (doc, _) = job(&d.su, t, "").await;
    revision(&d.su, t, doc, 2, "Uploaded").await;
    let (a, b) = (claim(d).await, claim(d).await);
    let (one, two) = if a.revision == 1 { (a, b) } else { (b, a) };
    (doc, one, two)
}

/// TC-49-03, and the scenario "Two jobs of one document" in each order.
#[tokio::test]
async fn tc_49_03_the_job_of_an_older_revision_ends_late() {
    let d = db().await;
    let (doc, one, two) = two_claims(&d).await;
    end(&d.worker, &two, text("two")).await;
    let End::Done(job) = end(&d.worker, &one, text("one")).await else {
        panic!("the lease holds");
    };
    assert_eq!(job.state, JobState::Done);
    assert_eq!(rev(&d.su, doc, 1).await.0, "Superseded");
    assert!(parts(&d.su, doc, 1).await.is_empty());
    assert_eq!(rev(&d.su, doc, 2).await.0, "Ready");
    assert_eq!(ready_revision(&d.su, doc).await, Some(2));

    let (doc, one, two) = two_claims(&d).await;
    end(&d.worker, &one, text("one")).await;
    end(&d.worker, &two, text("two")).await;
    assert_eq!(rev(&d.su, doc, 1).await.0, "Superseded");
    assert_eq!(rev(&d.su, doc, 2).await.0, "Ready");
    assert_eq!(parts(&d.su, doc, 2).await, ["two"]);
}

/// Scenario "Two jobs of one document": the two ends at the same time wait at the lock of the
/// document.
#[tokio::test]
async fn two_jobs_of_one_document_end_at_the_same_time() {
    let d = db().await;
    for _ in 0..5 {
        let (doc, one, two) = two_claims(&d).await;
        tokio::join!(
            end(&d.worker, &one, text("one")),
            end(&d.worker, &two, text("two"))
        );
        let ready =
            d.su.query_one(
                "select array_agg(revision) from revisions \
                 where document_id = $1 and state = 'Ready'",
                &[&doc],
            )
            .await
            .unwrap()
            .get::<_, Vec<i32>>(0);
        assert_eq!(ready, [2]);
        assert_eq!(rev(&d.su, doc, 1).await.0, "Superseded");
        assert!(parts(&d.su, doc, 1).await.is_empty());
    }
}

/// Scenario "A lease_token that changed".
#[tokio::test]
async fn a_lease_token_that_changed() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (doc, id) = job(&d.su, t, "").await;
    let c = claim(&d).await;
    d.su.execute(
        "update jobs set lease_token = gen_random_uuid() where job_id = $1",
        &[&id],
    )
    .await
    .unwrap();
    assert_eq!(end(&d.worker, &c, text("one")).await, End::LeaseLost);
    assert_eq!(rev(&d.su, doc, 1).await.0, "Uploaded");
    assert!(parts(&d.su, doc, 1).await.is_empty());
    assert_eq!(ready_revision(&d.su, doc).await, None);
}

/// TC-4-07
#[tokio::test]
async fn tc_4_07_the_decision_for_a_job() {
    let d = db().await;
    let (a, b) = (tenant(&d.su).await, tenant(&d.su).await);
    job(&d.su, a, "").await;
    let claim = ClaimTx::claim(&d.worker).await.unwrap().unwrap();
    assert_eq!(claim.decision_on(a), Decision::Allow);
    assert!(matches!(claim.decision_on(b), Decision::Deny(_)));
}

/// A worker with the hostile program and short values.
fn worker(d: &Db, max_object: u64) -> Worker {
    Worker {
        pool: d.worker.clone(),
        storage: storage(),
        sandbox: sandbox(
            &hostile(),
            Limits {
                wall: Duration::from_secs(10),
                ..LIMITS
            },
        ),
        settings: Settings {
            max_object,
            renew_every: Duration::from_millis(200),
        },
    }
}

/// Writes `body` as the object of a revision.
async fn object(tenant: Uuid, doc: Uuid, n: i32, body: &[u8]) {
    let s = storage();
    let (url, _) = s.sign_put(
        &strata::storage::key(tenant, doc, n),
        Duration::from_secs(60),
    );
    assert_eq!(put(&url, body, body.len(), Duration::ZERO).await, 200);
}

/// TC-47-16: the job of a `Superseded` revision does no extraction. The test
/// `a_finish_of_a_revision_in_a_later_state` of `upload.rs` checks the finish of such a
/// revision.
#[tokio::test]
async fn tc_47_16_a_superseded_revision_does_no_extraction() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (doc, id) = job(&d.su, t, "").await;
    d.su.execute(
        "update revisions set state = 'Superseded' where document_id = $1",
        &[&doc],
    )
    .await
    .unwrap();
    // No object: an extraction would fail the job.
    let End::Done(job) = worker(&d, 100).run_one().await.unwrap().unwrap() else {
        panic!("no lease to lose");
    };
    assert_eq!(job.state, JobState::Done);
    assert_eq!(row(&d.su, id).await.0, "Done");
    assert_eq!(rev(&d.su, doc, 1).await.0, "Superseded");
    let jobs: i64 =
        d.su.query_one("select count(*) from jobs", &[])
            .await
            .unwrap()
            .get(0);
    assert_eq!(jobs, 1, "no new job");
}

/// TC-5-09
#[tokio::test]
async fn tc_5_09_a_second_run() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (doc, id) = job(&d.su, t, "").await;
    let c = claim(&d).await;
    end(&d.worker, &c, text("one")).await;
    let (revision, text_parts) = (rev(&d.su, doc, 1).await, parts(&d.su, doc, 1).await);
    d.su.execute(
        "update jobs set state = 'Waiting', run_after = now() - interval '1 second', \
         ended_at = null where job_id = $1",
        &[&id],
    )
    .await
    .unwrap();
    worker(&d, 100).run_one().await.unwrap().unwrap();
    assert_eq!(row(&d.su, id).await.0, "Done");
    assert_eq!(rev(&d.su, doc, 1).await, revision);
    assert_eq!(parts(&d.su, doc, 1).await, text_parts);
}

/// TC-3-17
#[tokio::test]
async fn tc_3_17_a_job_that_names_a_document_of_b() {
    let d = db().await;
    let (a, b) = (tenant(&d.su).await, tenant(&d.su).await);
    let (doc_b, _) = document(&d.su, b, "Uploaded").await;
    let id = job_of(&d.su, a, doc_b, 1).await;
    let before = rev(&d.su, doc_b, 1).await;
    worker(&d, 100).run_one().await.unwrap().unwrap();
    assert_eq!(row(&d.su, id).await.0, "Done", "no document, no extraction");
    assert_eq!(rev(&d.su, doc_b, 1).await, before);
    assert!(parts(&d.su, doc_b, 1).await.is_empty());
}

/// The job is `Waiting` with the first backoff, and the revision does not change.
async fn assert_failed_run(d: &Db, id: Uuid, doc: Uuid) {
    let r =
        d.su.query_one(
            "select state, attempts, last_error, \
             run_after > now() + interval '25 seconds' from jobs where job_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    assert_eq!(
        (
            r.get::<_, &str>(0),
            r.get::<_, i32>(1),
            r.get::<_, &str>(2),
            r.get::<_, bool>(3)
        ),
        ("Waiting", 1, "JobFailed", true)
    );
    let (state, mark, class, ..) = rev(&d.su, doc, 1).await;
    assert_eq!((state.as_str(), mark, class), ("Uploaded", false, None));
}

/// Scenario "An object that does not exist".
#[tokio::test]
async fn an_object_that_does_not_exist() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (doc, id) = job(&d.su, t, "").await;
    worker(&d, 100).run_one().await.unwrap().unwrap();
    assert_failed_run(&d, id, doc).await;
}

/// Scenario "An object above 100 MB", with a maximum of 10 bytes. The test
/// `the_worker_reads_at_most_the_limit_and_one_byte` of `storage.rs` checks the read.
#[tokio::test]
async fn an_object_above_the_maximum() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (doc, id) = job(&d.su, t, "").await;
    object(t, doc, 1, b"forever and more").await;
    worker(&d, 10).run_one().await.unwrap().unwrap();
    assert_failed_run(&d, id, doc).await;
}

/// TC-5-04
#[tokio::test]
async fn tc_5_04_a_job_that_fails_at_each_run() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (_, id) = job(&d.su, t, "").await;
    let w = worker(&d, 100);
    for _ in 0..MAX_ATTEMPTS {
        w.run_one().await.unwrap().expect("the job is due");
        d.su.execute(
            "update jobs set run_after = now() - interval '1 second' where job_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    }
    assert_eq!(
        row(&d.su, id).await,
        ("Parked".into(), 5, "JobFailed".into())
    );
    assert!(w.run_one().await.unwrap().is_none(), "no worker claims it");
}

/// A crash in the sandbox sets the retry mark and runs again. The second crash sets `Failed`.
#[tokio::test]
async fn a_crash_runs_one_more_time() {
    let d = db().await;
    let t = tenant(&d.su).await;
    let (doc, id) = job(&d.su, t, "").await;
    object(t, doc, 1, b"crash").await;
    let w = worker(&d, 100);
    w.run_one().await.unwrap().unwrap();
    assert!(rev(&d.su, doc, 1).await.1, "the retry mark");
    assert_eq!(row(&d.su, id).await.0, "Waiting");
    d.su.execute(
        "update jobs set run_after = now() where job_id = $1",
        &[&id],
    )
    .await
    .unwrap();
    w.run_one().await.unwrap().unwrap();
    let (state, _, class, ..) = rev(&d.su, doc, 1).await;
    assert_eq!(
        (state.as_str(), class.as_deref()),
        ("Failed", Some("ExtractionCrash"))
    );
    assert_eq!(row(&d.su, id).await.0, "Done");
}
