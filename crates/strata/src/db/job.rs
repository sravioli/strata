//! The queries of the worker: the claim of a job, the renewal of its lease, and the tenant
//! transaction of the job (tech designs 0004, 0005 and 0049).
//!
//! `jobs` is a global table. Only `ClaimTx::into_tenant`, with an "allow" decision of
//! `authorize` for `ProcessDocument`, starts the tenant transaction of a job. `ClaimTx::resume`
//! opens it again for the end of the same job, and `end_job` writes only with its token.

use std::time::Duration;
use tracing::Instrument;

use strata_core::authorization::{Action, Decision, Principal, Resource, authorize};
use strata_core::job::Event as JobEvent;
use strata_core::job::{ErrorClass, Event, Job, JobState, LEASE_SECONDS, job_step};
use strata_core::revision::{Event as RevisionEvent, Revision, RevisionState, revision_step};

use crate::extraction::Part;
use uuid::Uuid;

use super::{Error, Params, Pool, READ_TENANT, TenantTx, Tx};

/// One due `ProcessDocument` job: `Waiting` after its `run_after`, or `Running` after its lease.
/// Times are whole seconds, as `job_step` has them.
// shortcut: one scan of the due jobs with `or`; use the partial indexes of 0005 with two
// statements if the claim gets slow with many jobs.
const CLAIM: &str = "select job_id, tenant_id, document_id, revision, state, attempts, \
     floor(extract(epoch from run_after))::bigint, \
     coalesce(floor(extract(epoch from lease_until))::bigint, 0), \
     coalesce(floor(extract(epoch from ended_at))::bigint, 0), last_error, \
     floor(extract(epoch from now()))::bigint, lease_token, traceparent \
     from jobs where kind = 'ProcessDocument' \
     and (state = 'Waiting' and run_after <= now() or state = 'Running' and lease_until <= now()) \
     order by run_after limit 1 for update skip locked";

const SET_TENANT: &str = "select set_config('app.tenant_id', $1::text, true)";

/// The result of `job_step` under the row lock of the claim, for the token that the claim read
/// (0005). A `Running` job gets a new random `lease_token`. Each other state gets none, so no
/// earlier worker can write. A time of 0 is null.
const WRITE_CLAIM: &str = "update jobs set state = $2, attempts = $3, run_after = to_timestamp($4), \
     lease_until = case when $5::float8 > 0 then to_timestamp($5) end, \
     ended_at = case when $6::float8 > 0 then to_timestamp($6) end, last_error = $7, \
     lease_token = case when $2 = 'Running' then gen_random_uuid() end \
     where job_id = $1 and lease_token is not distinct from $8 returning lease_token";

/// The count and the age of the oldest `Waiting` job of each kind (0133). Rows only: no tenant.
const QUEUE: &str = "select kind, count(*), \
     coalesce(extract(epoch from now() - min(run_after)), 0)::float8 \
     from jobs where state = 'Waiting' group by kind";

/// A renewal writes only for the token of the claim (0005).
const RENEW: &str = "update jobs set lease_until = now() + make_interval(secs => $3) \
     where job_id = $1 and lease_token = $2 and state = 'Running'";

/// The job that a worker claimed: its row after `job_step`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claimed {
    pub job_id: Uuid,
    pub tenant_id: Uuid,
    pub document_id: Uuid,
    pub revision: i32,
    pub job: Job,
    /// The token of the lease when the job is `Running`.
    pub lease_token: Option<Uuid>,
    /// The time of the claim, in seconds (`now()` of PostgreSQL).
    pub now: u64,
    /// The trace context of the request that queued the job (0133), or `None`.
    pub traceparent: Option<String>,
}

/// The columns of a job row that `job_step` gives, as SQL parameters. The names are the names of
/// the Rust variants, and a time of 0 is null.
pub(crate) struct JobRow {
    pub state: String,
    pub attempts: i32,
    pub run_after: f64,
    pub lease_until: f64,
    pub ended_at: f64,
    pub last_error: String,
}

impl JobRow {
    pub(crate) fn of(j: &Job) -> JobRow {
        JobRow {
            state: format!("{:?}", j.state),
            // `job_step` stops at `MAX_ATTEMPTS`, far below `i32::MAX`.
            attempts: i32::try_from(j.attempts).unwrap_or(i32::MAX),
            run_after: j.run_after as f64,
            lease_until: j.lease_until as f64,
            ended_at: j.ended_at as f64,
            last_error: format!("{:?}", j.last_error),
        }
    }
}

/// The principal of a job of `tenant` (0004).
fn job_principal(tenant: Uuid, tenant_active: bool) -> Principal {
    Principal::Job {
        tenant: tenant.as_u128(),
        tenant_active,
    }
}

/// The transaction of a claim, in the tenant context of the job when the job is `Running`.
pub struct ClaimTx {
    tx: Tx,
    claimed: Claimed,
    principal: Principal,
}

impl ClaimTx {
    /// Claims one due job, or gives `None` when no job is due. The statements that need no
    /// earlier result go together (the pipelining rule of 0049).
    ///
    /// A `Running` job with an expired lease first gets `LeaseExpired`. Then a `Waiting` job gets
    /// `Claim` with the status of its tenant, read in its tenant context.
    pub async fn claim(pool: &Pool) -> Result<Option<ClaimTx>, Error> {
        let tx = Tx(Some(pool.get().await?));
        let c = tx.conn();
        let (claim, set, tenant, write) = tokio::try_join!(
            c.prepare_cached(CLAIM),
            c.prepare_cached(SET_TENANT),
            c.prepare_cached(READ_TENANT),
            c.prepare_cached(WRITE_CLAIM),
        )?;
        let (_, row) = tokio::try_join!(
            c.batch_execute("begin"),
            c.query_opt(&claim, &[])
                .instrument(crate::db::span("claim"))
        )?;
        let Some(row) = row else {
            tx.end("rollback").await?;
            return Ok(None);
        };
        let now = secs(row.get(10));
        let read_token: Option<Uuid> = row.get(11);
        let mut claimed = Claimed {
            job_id: row.get(0),
            tenant_id: row.get(1),
            document_id: row.get(2),
            // A `ProcessDocument` job names a revision (0005). Revisions start at 1, so a row
            // with no revision gets 0: no revision is found, and the job ends with no
            // extraction (0049).
            revision: row.get::<_, Option<i32>>(3).unwrap_or(0),
            job: Job {
                state: parse_job_state(row.get(4)),
                attempts: u32::try_from(row.get::<_, i32>(5)).unwrap_or(0),
                run_after: secs(row.get(6)),
                lease_until: secs(row.get(7)),
                ended_at: secs(row.get(8)),
                last_error: parse_job_error(row.get(9)),
            },
            lease_token: None,
            now,
            traceparent: row.get(12),
        };
        let mut tenant_active = false;
        if claimed.job.state == JobState::Running {
            claimed.job = job_step(claimed.job, Event::LeaseExpired, now);
        }
        if claimed.job.state == JobState::Waiting {
            let tenant_text = claimed.tenant_id.to_string();
            let p: Params = &[&tenant_text];
            let (_, status) = tokio::try_join!(
                c.query_one(&set, p)
                    .instrument(crate::db::span("set_tenant")),
                c.query_opt(&tenant, &[])
                    .instrument(crate::db::span("read_tenant"))
            )?;
            tenant_active = status.is_some_and(|r| r.get::<_, bool>(0));
            claimed.job = job_step(claimed.job, Event::Claim(tenant_active), now);
        }
        let j = JobRow::of(&claimed.job);
        let p: Params = &[
            &claimed.job_id,
            &j.state,
            &j.attempts,
            &j.run_after,
            &j.lease_until,
            &j.ended_at,
            &j.last_error,
            &read_token,
        ];
        claimed.lease_token = c
            .query_one(&write, p)
            .instrument(crate::db::span("write_claim"))
            .await?
            .get(0);
        let principal = job_principal(claimed.tenant_id, tenant_active);
        Ok(Some(ClaimTx {
            tx,
            claimed,
            principal,
        }))
    }

    pub fn claimed(&self) -> &Claimed {
        &self.claimed
    }

    /// The decision for the job on the documents of its own tenant (0004).
    pub fn decision(&self) -> Decision {
        self.decision_on(self.claimed.tenant_id)
    }

    /// The decision for the job on a document of `tenant`. Only the tenant of the job gets
    /// "allow" (TC-4-07).
    pub fn decision_on(&self, tenant: Uuid) -> Decision {
        let resource = Resource {
            tenant: tenant.as_u128(),
        };
        authorize(self.principal, Action::ProcessDocument, resource)
    }

    /// The tenant transaction of a claimed job again, for `end_job` after the run. Only a
    /// `Running` job has a token, and `into_tenant` started it with "allow": a job that runs
    /// continues to its end (TC-4-10). For each other claim, `end_job` writes nothing.
    pub async fn resume(pool: &Pool, claimed: &Claimed) -> Result<TenantTx, Error> {
        let tx = Tx(Some(pool.get().await?));
        let c = tx.conn();
        let set = c.prepare_cached(SET_TENANT).await?;
        let tenant_text = claimed.tenant_id.to_string();
        let p: Params = &[&tenant_text];
        tokio::try_join!(
            c.batch_execute("begin"),
            c.query_one(&set, p)
                .instrument(crate::db::span("set_tenant"))
        )?;
        Ok(TenantTx {
            tx,
            tenant_id: claimed.tenant_id,
        })
    }

    /// The tenant transaction of a `Running` job, from an "allow" decision only. Each other case
    /// commits the result of the claim and gives `None`: the worker does not start the job.
    pub async fn into_tenant(self) -> Result<Option<TenantTx>, Error> {
        let running = self.claimed.job.state == JobState::Running;
        if !(running && matches!(self.decision(), Decision::Allow)) {
            self.tx.end("commit").await?;
            return Ok(None);
        }
        Ok(Some(TenantTx {
            tx: self.tx,
            tenant_id: self.claimed.tenant_id,
        }))
    }
}

/// Renews the lease of a claimed job. False when the token is no longer the token of the job:
/// then a different worker claimed it, and the worker stops the extraction (0005).
pub async fn renew(pool: &Pool, job_id: Uuid, lease_token: Uuid) -> Result<bool, Error> {
    let c = pool.get().await?;
    let st = c.prepare_cached(RENEW).await?;
    let secs = LEASE_SECONDS as f64;
    Ok(c.execute(&st, &[&job_id, &lease_token, &secs])
        .instrument(crate::db::span("renew"))
        .await?
        == 1)
}

/// Renews the lease each `every`, and ends at the first renewal that writes nothing. The worker
/// gives it to `Sandbox::run` as the cancel, so the loss of the lease stops the extraction. A
/// failed renewal tries again at the next interval: the lease holds 300 s, so one failure loses
/// nothing.
pub async fn keep_lease(pool: &Pool, job_id: Uuid, lease_token: Uuid, every: Duration) {
    loop {
        tokio::time::sleep(every).await;
        match renew(pool, job_id, lease_token).await {
            Ok(true) => {}
            Ok(false) => return,
            Err(e) => tracing::warn!(%job_id, error = e.class(), "a renewal of the lease"),
        }
    }
}

/// What the end of a job writes.
#[derive(Debug, PartialEq, Eq)]
pub enum Finish {
    /// The event of the extraction for `revision_step`, with the text parts for `Extracted`.
    Extraction(RevisionEvent, Vec<Part>),
    /// No extraction: the revision is not found, or `may_extract` is false. `Succeed`.
    NoExtraction,
    /// A failure of the storage or of the sandbox: `Fail`, with no change to the revision.
    Failure,
}

/// The end of a job, or the loss of its lease.
#[derive(Debug, PartialEq, Eq)]
pub enum End {
    Done(Job),
    /// The `lease_token` changed after the claim: the transaction rolled back.
    LeaseLost,
}

const READ_REVISION: &str = "select state, retry_mark, error_class, no_text, etag from revisions \
     where tenant_id = $1 and document_id = $2 and revision = $3";

/// The revision of a claimed job, for `may_extract` at the claim, and the `ETag` that its finish
/// checked (null for a finish before V6). `None` when the tenant of the
/// job has no such revision (TC-3-17). No lock: `end_job` locks the document first and then the
/// revision, and a lock here before the document could deadlock with it.
pub async fn read_revision(
    tx: &mut TenantTx,
    claimed: &Claimed,
) -> Result<Option<(Revision, Option<String>)>, Error> {
    let tenant = tx.tenant_id;
    let c = tx.tx.conn();
    let st = c.prepare_cached(READ_REVISION).await?;
    let p: Params = &[&tenant, &claimed.document_id, &claimed.revision];
    Ok(c.query_opt(&st, p)
        .instrument(crate::db::span("read_revision"))
        .await?
        .map(|row| (super::upload::revision_of(&row), row.get(4))))
}

const LOCK_DOCUMENT: &str = "select floor(extract(epoch from now()))::bigint from documents \
     where tenant_id = $1 and document_id = $2 for update";

/// The writes of the end of a job, in one statement (the pipelining rule of 0049). Each write
/// runs only when the update of the job finds the token of the claim (0005), so a lost lease
/// writes nothing. For a new `Ready` state (`$11`): the text parts, `ready_revision`, the state
/// that `revision_step` gives for `NewerReady` (`$19`) on each older revision that the event
/// changes (`$21`), and the removal of the text parts of those revisions. `$14` is true when
/// `revision_step` changed the revision.
const END_JOB: &str = "with j as (update jobs set state = $3, attempts = $4, \
     run_after = to_timestamp($5), \
     lease_until = case when $20::float8 > 0 then to_timestamp($20) end, \
     ended_at = case when $6::float8 > 0 then to_timestamp($6) end, \
     last_error = $7, lease_token = null \
     where job_id = $1 and lease_token = $2 returning job_id), \
     parts as (insert into text_parts \
     (tenant_id, document_id, revision, part, location_kind, location_number, part_text) \
     select $8, $9, $10, p.n::integer, 'page', p.location, p.part_text \
     from unnest($12::integer[], $13::text[]) with ordinality as p (location, part_text, n) \
     where $11 and exists (select from j)), \
     ready as (update documents set ready_revision = $10 \
     where tenant_id = $8 and document_id = $9 and $11 and exists (select from j)), \
     older as (update revisions set state = $19, status_at = now() \
     where tenant_id = $8 and document_id = $9 and revision < $10 \
     and state = any($21::text[]) and $11 and exists (select from j)), \
     old_parts as (delete from text_parts \
     where tenant_id = $8 and document_id = $9 and revision < $10 \
     and $11 and exists (select from j)), \
     rev as (update revisions set state = $15, retry_mark = $16, error_class = $17, \
     no_text = $18, status_at = case when state = $15 then status_at else now() end \
     where tenant_id = $8 and document_id = $9 and revision = $10 \
     and $14 and exists (select from j)) \
     select count(*) from j";

/// The states of an older revision that `NewerReady` changes (0049), and the state that
/// `revision_step` gives them. The SQL writes this result; it does not decide the change.
fn superseded() -> (Vec<String>, String) {
    const STATES: [RevisionState; 5] = [
        RevisionState::AwaitingUpload,
        RevisionState::Uploaded,
        RevisionState::Ready,
        RevisionState::Failed,
        RevisionState::Superseded,
    ];
    let step = |state| {
        let r = Revision {
            state,
            retry_mark: false,
            error_class: None,
            no_text: false,
        };
        revision_step(r, RevisionEvent::NewerReady).state
    };
    let changed: Vec<RevisionState> = STATES.into_iter().filter(|s| step(*s) != *s).collect();
    // The Lean model gives one result for each of these states.
    let new = step(changed[0]);
    debug_assert!(changed.iter().all(|s| step(*s) == new));
    (
        changed.iter().map(|s| format!("{s:?}")).collect(),
        format!("{new:?}"),
    )
}

/// The transaction that ends a job (0049). It locks the document, so two jobs of one document
/// end one after the other. A `lease_token` that changed writes nothing.
///
/// The two locks go together, in that order, and the `commit` goes with `END_JOB` (the
/// pipelining rule of 0049). With a lost lease the statement writes nothing, so that `commit`
/// changes nothing.
pub async fn end_job(tx: TenantTx, claimed: &Claimed, result: Finish) -> Result<End, Error> {
    let (tenant, doc, rev) = (tx.tenant_id, claimed.document_id, claimed.revision);
    let (now, locked) = {
        let c = tx.tx.conn();
        let (lock_doc, lock_rev) = tokio::try_join!(
            c.prepare_cached(LOCK_DOCUMENT),
            c.prepare_cached(super::upload::LOCK_REVISION),
        )?;
        let d: Params = &[&tenant, &doc];
        let r: Params = &[&tenant, &doc, &rev];
        let (doc_row, rev_row) = tokio::try_join!(
            c.query_opt(&lock_doc, d)
                .instrument(crate::db::span("lock_document")),
            c.query_opt(&lock_rev, r)
                .instrument(crate::db::span("lock_revision"))
        )?;
        let now = doc_row.map_or(claimed.now, |row| secs(row.get(0)));
        (now, rev_row.map(|row| super::upload::revision_of(&row)))
    };
    let mut event = JobEvent::Succeed;
    let (mut ready, mut changed) = (false, false);
    let (mut locations, mut texts): (Vec<i32>, Vec<&str>) = (Vec::new(), Vec::new());
    let mut new = locked;
    if let (Some(old), Finish::Extraction(e, parts)) = (locked, &result) {
        let next = revision_step(old, *e);
        if next.retry_mark && !old.retry_mark {
            event = JobEvent::Fail;
        }
        ready = next.state == RevisionState::Ready && old.state != RevisionState::Ready;
        if ready {
            locations = parts
                .iter()
                .map(|p| i32::try_from(p.location).unwrap_or(i32::MAX))
                .collect();
            texts = parts.iter().map(|p| p.text.as_str()).collect();
        }
        changed = next != old;
        new = Some(next);
    }
    if result == Finish::Failure {
        event = JobEvent::Fail;
    }
    let job = job_step(claimed.job, event, now);
    let j = JobRow::of(&job);
    // No revision: `changed` is false, so the statement writes no revision.
    let new = new.unwrap_or(Revision {
        state: RevisionState::Uploaded,
        retry_mark: false,
        error_class: None,
        no_text: false,
    });
    let rev_state = format!("{:?}", new.state);
    let class = new.error_class.map(|c| format!("{c:?}"));
    let (older, superseded) = superseded();
    let p: Params = &[
        // The job, $1 to $7, and its `lease_until` at $20.
        &claimed.job_id,
        &claimed.lease_token,
        &j.state,
        &j.attempts,
        &j.run_after,
        &j.ended_at,
        &j.last_error,
        // The revision, $8 to $10.
        &tenant,
        &doc,
        &rev,
        // A new `Ready` state and its text parts, $11 to $13.
        &ready,
        &locations,
        &texts,
        // The result of `revision_step`, $14 to $18.
        &changed,
        &rev_state,
        &new.retry_mark,
        &class,
        &new.no_text,
        // `NewerReady` on the older revisions: the new state at $19, the states it changes at $21.
        &superseded,
        &j.lease_until,
        &older,
    ];
    let rows = tx.query_and_end(END_JOB, "end_job", p, true).await?;
    let ended: i64 = rows.first().map_or(0, |r| r.get(0));
    Ok(if ended == 1 {
        End::Done(job)
    } else {
        End::LeaseLost
    })
}

fn secs(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

// The names in the database are the names of the Rust variants. The check constraints of V2
// hold the same lists, so another text cannot come back.
fn parse_job_state(s: &str) -> JobState {
    match s {
        "Waiting" => JobState::Waiting,
        "Running" => JobState::Running,
        "Parked" => JobState::Parked,
        "Done" => JobState::Done,
        other => unreachable!("a state that the check constraint refuses: {other}"),
    }
}

fn parse_job_error(s: &str) -> ErrorClass {
    match s {
        "NoError" => ErrorClass::NoError,
        "TenantNotActive" => ErrorClass::TenantNotActive,
        "LeaseExpired" => ErrorClass::LeaseExpired,
        "JobFailed" => ErrorClass::JobFailed,
        other => unreachable!("an error class that the check constraint refuses: {other}"),
    }
}

/// The rows of `QUEUE` for each kind, 0 for a kind with no `Waiting` job. A job that waits for
/// its retry has an age of 0.
pub async fn queue(pool: &Pool) -> Result<Vec<(&'static str, u64, f64)>, Error> {
    let c = pool.get().await?;
    let st = c.prepare_cached(QUEUE).await?;
    let rows = c.query(&st, &[]).await?;
    Ok(["ProcessDocument", "RemoveFiles"]
        .into_iter()
        .map(|kind| {
            let row = rows.iter().find(|r| r.get::<_, &str>(0) == kind);
            let waiting = row.map_or(0, |r| u64::try_from(r.get::<_, i64>(1)).unwrap_or(0));
            let age = row.map_or(0.0, |r| r.get::<_, f64>(2).max(0.0));
            (kind, waiting, age)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_go_both_ways() {
        use JobState::*;
        for s in [Waiting, Running, Parked, Done] {
            assert_eq!(parse_job_state(&format!("{s:?}")), s);
        }
        use ErrorClass::*;
        for e in [NoError, TenantNotActive, LeaseExpired, JobFailed] {
            assert_eq!(parse_job_error(&format!("{e:?}")), e);
        }
    }
}
