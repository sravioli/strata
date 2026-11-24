//! One run of a `ProcessDocument` job, from the claim to the transaction that ends it (tech
//! designs 0005 and 0049).

use std::sync::Arc;
use std::time::Duration;

use strata_core::job::JobState;
use strata_core::revision::{Event, may_extract};
use tracing::Instrument;
use tracing::field::Empty;

use crate::cleanup;
use crate::db::job::{self, ClaimTx, End, Finish};
use crate::db::{Error, Pool};
use crate::extraction::{Outcome, class, outcome};
use crate::sandbox::Sandbox;
use crate::storage::{self, Storage};
use crate::telemetry::Metrics;
use crate::upload::MAX_FILE_BYTES;

/// The values of 0049 in one value, so that a test can set them lower.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// The largest object that the worker extracts. It reads at most one byte more.
    pub max_object: u64,
    /// The interval of the renewal of the lease.
    pub renew_every: Duration,
    /// The wait after the first empty poll. It doubles after each empty poll, up to `poll_max`.
    pub poll: Duration,
    pub poll_max: Duration,
    /// The interval of the cleanup of `Done` jobs.
    pub cleanup_every: Duration,
    /// The interval of the read of the queue for the metrics (0133).
    pub queue_every: Duration,
}

/// 100 MB (0006), a renewal each 100 seconds for the lease of 300 seconds (0005), a poll of 1 to
/// 10 seconds and a cleanup each 60 seconds (0049).
pub const SETTINGS: Settings = Settings {
    max_object: MAX_FILE_BYTES as u64,
    renew_every: Duration::from_secs(100),
    poll: Duration::from_secs(1),
    poll_max: Duration::from_secs(10),
    cleanup_every: cleanup::EVERY,
    queue_every: Duration::from_secs(15),
};

/// The kind of the jobs that the worker runs.
const KIND: &str = "ProcessDocument";

pub struct Worker {
    pub pool: Pool,
    pub storage: Storage,
    pub sandbox: Sandbox,
    pub settings: Settings,
    pub metrics: Arc<Metrics>,
}

impl Worker {
    /// The loop of `strata worker`: at most 2 jobs at the same time (0049), one in each of two
    /// polls, and the cleanup of `Done` jobs. It runs until the caller drops it.
    pub async fn run(&self) {
        tokio::join!(self.poll(), self.poll(), self.cleanup(), self.queue());
    }

    /// Runs one job after the other. With no job, it waits before the next claim.
    async fn poll(&self) {
        let mut wait = self.settings.poll;
        loop {
            match self.run_one().await {
                Ok(Some(End::Done(job))) => {
                    tracing::info!(state = ?job.state, last_error = ?job.last_error, "a job ended");
                    wait = self.settings.poll;
                    continue;
                }
                Ok(Some(End::LeaseLost)) => {
                    tracing::warn!("the lease of a job was lost");
                    wait = self.settings.poll;
                    continue;
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(error = e.class(), "a run of a job"),
            }
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(self.settings.poll_max);
        }
    }

    /// `cleanup::remove_done_jobs` at each interval. A failed run goes to the log, and the next
    /// run continues.
    async fn cleanup(&self) {
        let mut every = tokio::time::interval(self.settings.cleanup_every);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            let run = async {
                let c = self.pool.get().await.map_err(Error::from)?;
                cleanup::remove_done_jobs(&c).await.map_err(Error::from)
            };
            match run.await {
                Ok(0) => {}
                Ok(n) => tracing::info!(rows = n, "removed Done jobs"),
                Err(e) => tracing::warn!(error = e.class(), "cleanup of jobs failed"),
            }
        }
    }

    /// The count and the age of the oldest `Waiting` job of each kind, for the metrics.
    async fn queue(&self) {
        let mut every = tokio::time::interval(self.settings.queue_every);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            match job::queue(&self.pool).await {
                Ok(kinds) => {
                    for (kind, waiting, age) in kinds {
                        self.metrics.queue(kind, waiting, age);
                    }
                }
                Err(e) => tracing::warn!(error = e.class(), "the read of the queue failed"),
            }
        }
    }

    /// Claims one due job and runs it to its end. `None` when no job is due. A job that the claim
    /// does not start (a tenant that is not active, the maximum of attempts) gives its new row.
    ///
    /// The run has a span for the job, a child of the `traceparent` of the row, and a child span
    /// for the attempt (0133).
    pub async fn run_one(&self) -> Result<Option<End>, Error> {
        let Some(claim) = ClaimTx::claim(&self.pool).await? else {
            return Ok(None);
        };
        let claimed = claim.claimed().clone();
        let job = tracing::info_span!(
            "job",
            otel.name = "job ProcessDocument",
            strata.job_id = %claimed.job_id,
            strata.tenant_id = %claimed.tenant_id,
            strata.document_id = %claimed.document_id,
            strata.revision = claimed.revision,
        );
        crate::telemetry::set_parent(&job, claimed.traceparent.as_deref());
        let attempt = tracing::info_span!(
            parent: &job,
            "attempt",
            strata.attempt = claimed.job.attempts,
            strata.job_id = %claimed.job_id,
            strata.result = Empty,
        );
        let end = self
            .attempt(claim, claimed)
            .instrument(attempt.clone())
            .await;
        let result = match &end {
            Ok(Some(End::Done(j))) => match j.state {
                JobState::Waiting => "Waiting",
                JobState::Running => "Running",
                JobState::Parked => "Parked",
                JobState::Done => "Done",
            },
            Ok(Some(End::LeaseLost)) => "LeaseLost",
            Ok(None) => "None",
            Err(_) => "Error",
        };
        attempt.record("strata.result", result);
        self.metrics.attempt(KIND, result);
        end
    }

    async fn attempt(&self, claim: ClaimTx, claimed: job::Claimed) -> Result<Option<End>, Error> {
        let Some(mut tx) = claim.into_tenant().await? else {
            return Ok(Some(End::Done(claimed.job)));
        };
        // At the claim: no extraction for a revision that the tenant does not have, or that
        // `may_extract` refuses (0047). The job ends in the transaction of the claim.
        let revision = job::read_revision(&mut tx, &claimed).await?;
        if !revision.is_some_and(may_extract) {
            return job::end_job(tx, &claimed, Finish::NoExtraction)
                .await
                .map(Some);
        }
        tx.commit().await?;

        // A `Running` job always has a token. The nil token renews nothing, so the run stops.
        let token = claimed.lease_token.unwrap_or_default();
        let lost = job::keep_lease(&self.pool, claimed.job_id, token, self.settings.renew_every);
        tokio::pin!(lost);
        let key = storage::key(claimed.tenant_id, claimed.document_id, claimed.revision);
        let max = self.settings.max_object;
        let read = tokio::select! {
            r = self.storage.read(&key, max) => r,
            () = &mut lost => return Ok(Some(End::LeaseLost)),
        };
        let finish = match read {
            Ok(Some(bytes)) if bytes.len() as u64 <= max => {
                match self.extract(&bytes, lost.as_mut()).await {
                    Outcome::Text(parts) => {
                        Finish::Extraction(Event::Extracted(parts.is_empty()), parts)
                    }
                    Outcome::Failed(class) => {
                        Finish::Extraction(Event::ExtractFailed(class), Vec::new())
                    }
                    Outcome::SandboxFailure => Finish::Failure,
                    // The lease is lost: a different worker has the job, and nothing writes.
                    Outcome::Cancelled => return Ok(Some(End::LeaseLost)),
                }
            }
            // No object, or an object above the maximum: a failure of the job (0006).
            Ok(_) => Finish::Failure,
            Err(e) => {
                tracing::warn!(job_id = %claimed.job_id, error = %e, "the read of the object");
                Finish::Failure
            }
        };
        let tx = ClaimTx::resume(&self.pool, &claimed).await?;
        job::end_job(tx, &claimed, finish).await.map(Some)
    }

    /// The call of `strata-extract`, in its own span with the format, the run time, the exit
    /// class and the number of text parts. The span holds no text.
    async fn extract(
        &self,
        bytes: &[u8],
        cancel: impl std::future::Future<Output = ()>,
    ) -> Outcome {
        let format = match strata_extract::detect(bytes) {
            strata_extract::Format::Pdf => "pdf",
            strata_extract::Format::Unsupported => "unsupported",
        };
        let span = tracing::info_span!(
            "extraction",
            strata.format = format,
            strata.duration_s = Empty,
            strata.exit_class = Empty,
            strata.parts = Empty,
        );
        let start = std::time::Instant::now();
        let run = self
            .sandbox
            .run(bytes, cancel)
            .instrument(span.clone())
            .await;
        let secs = start.elapsed().as_secs_f64();
        let exit = class(&run.stop);
        span.record("strata.duration_s", secs);
        span.record("strata.exit_class", exit);
        self.metrics.extraction(format, exit, secs);
        let out = outcome(run);
        if let Outcome::Text(parts) = &out {
            span.record("strata.parts", parts.len());
        }
        out
    }
}
