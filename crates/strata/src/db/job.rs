//! The queries of the worker: the claim of a job, the renewal of its lease, and the tenant
//! transaction of the job (tech designs 0004, 0005 and 0049).
//!
//! `jobs` is a global table. Only `ClaimTx::into_tenant`, with an "allow" decision of
//! `authorize` for `ProcessDocument`, gives the tenant transaction of a job.

use std::time::Duration;

use strata_core::authorization::{Action, Decision, Principal, Resource, authorize};
use strata_core::job::Event as JobEvent;
use strata_core::job::{ErrorClass, Event, Job, JobState, LEASE_SECONDS, job_step};
use strata_core::revision::{Event as RevisionEvent, RevisionState, revision_step};

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
     floor(extract(epoch from now()))::bigint \
     from jobs where kind = 'ProcessDocument' \
     and (state = 'Waiting' and run_after <= now() or state = 'Running' and lease_until <= now()) \
     order by run_after limit 1 for update skip locked";

const SET_TENANT: &str = "select set_config('app.tenant_id', $1::text, true)";

/// The result of `job_step` under the row lock of the claim. A `Running` job gets a new random
/// `lease_token`. Each other state gets none, so no earlier worker can write.
const WRITE_CLAIM: &str = "update jobs set state = $2, attempts = $3, run_after = to_timestamp($4), \
     lease_until = case when $2 = 'Running' then to_timestamp($5) end, \
     ended_at = case when $6::float8 > 0 then to_timestamp($6) end, last_error = $7, \
     lease_token = case when $2 = 'Running' then gen_random_uuid() end \
     where job_id = $1 returning lease_token";

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
        let (_, row) = tokio::try_join!(c.batch_execute("begin"), c.query_opt(&claim, &[]))?;
        let Some(row) = row else {
            tx.end("rollback").await?;
            return Ok(None);
        };
        let now = secs(row.get(10));
        let mut claimed = Claimed {
            job_id: row.get(0),
            tenant_id: row.get(1),
            document_id: row.get(2),
            // A `ProcessDocument` job always names a revision (0005).
            revision: row.get::<_, Option<i32>>(3).unwrap_or(0),
            job: Job {
                state: parse_state(row.get(4)),
                attempts: u32::try_from(row.get::<_, i32>(5)).unwrap_or(0),
                run_after: secs(row.get(6)),
                lease_until: secs(row.get(7)),
                ended_at: secs(row.get(8)),
                last_error: parse_error(row.get(9)),
            },
            lease_token: None,
            now,
        };
        let mut tenant_active = false;
        if claimed.job.state == JobState::Running {
            claimed.job = job_step(claimed.job, Event::LeaseExpired, now);
        }
        if claimed.job.state == JobState::Waiting {
            let tenant_text = claimed.tenant_id.to_string();
            let p: Params = &[&tenant_text];
            let (_, status) = tokio::try_join!(c.query_one(&set, p), c.query_opt(&tenant, &[]))?;
            tenant_active = status.is_some_and(|r| r.get::<_, bool>(0));
            claimed.job = job_step(claimed.job, Event::Claim(tenant_active), now);
        }
        let j = &claimed.job;
        let (state, error) = (format!("{:?}", j.state), format!("{:?}", j.last_error));
        let attempts = i32::try_from(j.attempts).unwrap_or(i32::MAX);
        let (run_after, lease_until, ended_at) =
            (j.run_after as f64, j.lease_until as f64, j.ended_at as f64);
        let p: Params = &[
            &claimed.job_id,
            &state,
            &attempts,
            &run_after,
            &lease_until,
            &ended_at,
            &error,
        ];
        claimed.lease_token = c.query_one(&write, p).await?.get(0);
        let principal = Principal::Job {
            tenant: claimed.tenant_id.as_u128(),
            tenant_active,
        };
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

    /// A new transaction in the tenant context of a claimed job, for the end of the job. The
    /// tenant was active at the claim, and a job that runs continues to its end (TC-4-10).
    pub async fn resume(pool: &Pool, claimed: &Claimed) -> Result<ClaimTx, Error> {
        let tx = Tx(Some(pool.get().await?));
        let c = tx.conn();
        let set = c.prepare_cached(SET_TENANT).await?;
        let tenant_text = claimed.tenant_id.to_string();
        let p: Params = &[&tenant_text];
        tokio::try_join!(c.batch_execute("begin"), c.query_one(&set, p))?;
        let principal = Principal::Job {
            tenant: claimed.tenant_id.as_u128(),
            tenant_active: true,
        };
        Ok(ClaimTx {
            tx,
            claimed: claimed.clone(),
            principal,
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
    Ok(c.execute(&st, &[&job_id, &lease_token, &secs]).await? == 1)
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
            Err(e) => tracing::warn!(%job_id, error = %e, "a renewal of the lease"),
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

const READ_REVISION: &str = "select state, retry_mark, error_class, no_text from revisions \
     where tenant_id = $1 and document_id = $2 and revision = $3";

/// The revision of a claimed job, for `may_extract` at the claim. `None` when the tenant of the
/// job has no such revision (TC-3-17). No lock: `end_job` locks the document first and then the
/// revision, and a lock here before the document could deadlock with it.
pub async fn read_revision(
    tx: &mut TenantTx,
    document_id: Uuid,
    revision: i32,
) -> Result<Option<strata_core::revision::Revision>, Error> {
    let tenant = tx.tenant_id;
    let c = tx.tx.conn();
    let st = c.prepare_cached(READ_REVISION).await?;
    let row = c
        .query_opt(&st, &[&tenant, &document_id, &revision])
        .await?;
    Ok(row.map(|row| {
        let class: Option<&str> = row.get(2);
        strata_core::revision::Revision {
            state: super::upload::parse_state(row.get(0)),
            retry_mark: row.get(1),
            error_class: class.map(super::upload::parse_class),
            no_text: row.get(3),
        }
    }))
}

const LOCK_DOCUMENT: &str = "select floor(extract(epoch from now()))::bigint from documents \
     where tenant_id = $1 and document_id = $2 for update";

/// The writes of the end of a job, in one statement (the pipelining rule of 0049). Each write
/// runs only when the update of the job finds the token of the claim (0005), so a lost lease
/// writes nothing. For a new `Ready` state (`$11`): the text parts, `ready_revision`,
/// `NewerReady` of `revision_step` for each older revision that it changes, and the removal of
/// the text parts of those revisions. `$14` is true when `revision_step` changed the revision.
const END_JOB: &str = "with j as (update jobs set state = $3, attempts = $4, \
     run_after = to_timestamp($5), lease_until = null, \
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
     older as (update revisions set state = 'Superseded', status_at = now() \
     where tenant_id = $8 and document_id = $9 and revision < $10 \
     and state in ('AwaitingUpload', 'Uploaded', 'Ready') and $11 and exists (select from j)), \
     old_parts as (delete from text_parts \
     where tenant_id = $8 and document_id = $9 and revision < $10 \
     and $11 and exists (select from j)), \
     rev as (update revisions set state = $15, retry_mark = $16, error_class = $17, \
     no_text = $18, status_at = case when state = $15 then status_at else now() end \
     where tenant_id = $8 and document_id = $9 and revision = $10 \
     and $14 and exists (select from j)) \
     select count(*) from j";

/// The transaction that ends a job (0049). It locks the document, so two jobs of one document
/// end one after the other. A `lease_token` that changed writes nothing.
pub async fn end_job(mut tx: TenantTx, claimed: &Claimed, result: Finish) -> Result<End, Error> {
    let (tenant, doc, rev) = (tx.tenant_id, claimed.document_id, claimed.revision);
    let now = {
        let c = tx.tx.conn();
        let lock = c.prepare_cached(LOCK_DOCUMENT).await?;
        let p: Params = &[&tenant, &doc];
        c.query_opt(&lock, p)
            .await?
            .map_or(claimed.now, |row| secs(row.get(0)))
    };
    let locked = super::upload::lock_revision(&mut tx, doc, rev).await?;
    let mut event = JobEvent::Succeed;
    let (mut ready, mut changed) = (false, false);
    let (mut locations, mut texts): (Vec<i32>, Vec<&str>) = (Vec::new(), Vec::new());
    let mut new = locked.as_ref().map(|l| l.revision);
    if let (Some(locked), Finish::Extraction(e, parts)) = (&locked, &result) {
        let old = locked.revision;
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
    let (state, error) = (format!("{:?}", job.state), format!("{:?}", job.last_error));
    let attempts = i32::try_from(job.attempts).unwrap_or(i32::MAX);
    let (run_after, ended_at) = (job.run_after as f64, job.ended_at as f64);
    let new = new.unwrap_or(strata_core::revision::Revision {
        state: RevisionState::Uploaded,
        retry_mark: false,
        error_class: None,
        no_text: false,
    });
    let rev_state = format!("{:?}", new.state);
    let class = new.error_class.map(|c| format!("{c:?}"));
    let p: Params = &[
        &claimed.job_id,
        &claimed.lease_token,
        &state,
        &attempts,
        &run_after,
        &ended_at,
        &error,
        &tenant,
        &doc,
        &rev,
        &ready,
        &locations,
        &texts,
        &changed,
        &rev_state,
        &new.retry_mark,
        &class,
        &new.no_text,
    ];
    let c = tx.tx.conn();
    let st = c.prepare_cached(END_JOB).await?;
    let ended: i64 = c.query_one(&st, p).await?.get(0);
    if ended != 1 {
        tx.rollback().await?;
        return Ok(End::LeaseLost);
    }
    tx.commit().await?;
    Ok(End::Done(job))
}

fn secs(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

// The names in the database are the names of the Rust variants. The check constraints of V2
// hold the same lists, so another text cannot come back.
fn parse_state(s: &str) -> JobState {
    match s {
        "Waiting" => JobState::Waiting,
        "Running" => JobState::Running,
        "Parked" => JobState::Parked,
        "Done" => JobState::Done,
        other => unreachable!("a state that the check constraint refuses: {other}"),
    }
}

fn parse_error(s: &str) -> ErrorClass {
    match s {
        "NoError" => ErrorClass::NoError,
        "TenantNotActive" => ErrorClass::TenantNotActive,
        "LeaseExpired" => ErrorClass::LeaseExpired,
        "JobFailed" => ErrorClass::JobFailed,
        other => unreachable!("an error class that the check constraint refuses: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_go_both_ways() {
        use JobState::*;
        for s in [Waiting, Running, Parked, Done] {
            assert_eq!(parse_state(&format!("{s:?}")), s);
        }
        use ErrorClass::*;
        for e in [NoError, TenantNotActive, LeaseExpired, JobFailed] {
            assert_eq!(parse_error(&format!("{e:?}")), e);
        }
    }
}
