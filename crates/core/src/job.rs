//! The job state function (tech design 0005). The theorems of its "Proof obligations" table are
//! about `job_step` (`lean/Strata/Proofs/Job.lean`). The SQL of each step writes its result.
//! Proved subset of tech design 0009: times add with `saturating_add`, so no step overflows.

/// 5 runs (owner decision of 2026-11-18 in 0005).
pub const MAX_ATTEMPTS: u32 = 5;
/// The length of a lease, in seconds. 0005 leaves the value to implementation.
pub const LEASE_SECONDS: u64 = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Waiting,
    Running,
    Parked,
    Done,
}

/// A class from a fixed list, never an error text: a text can hold tenant content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorClass {
    NoError,
    TenantNotActive,
    LeaseExpired,
    JobFailed,
}

/// The fields of a job row that the state function reads and writes. Times are seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Job {
    pub state: JobState,
    pub attempts: u32,
    pub run_after: u64,
    pub lease_until: u64,
    pub ended_at: u64,
    pub last_error: ErrorClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A worker claims the job. The argument is the status of the tenant at the claim.
    Claim(bool),
    Succeed,
    Fail,
    LeaseExpired,
    /// The operator requeues a parked job.
    Requeue,
}

/// The backoff after the failed run number `n`: 30 s, 2 min, 10 min, 30 min (0005).
pub fn backoff_seconds(n: u32) -> u64 {
    match n {
        1 => 31,
        2 => 120,
        3 => 600,
        _ => 1800,
    }
}

/// The next state of a job. An event that does not apply leaves the job unchanged.
/// The parameter is `j`, not `job`: in Lean, `job` is the namespace of this module.
pub fn job_step(j: Job, event: Event, now: u64) -> Job {
    match (j.state, event) {
        (JobState::Waiting, Event::Claim(false)) => Job {
            state: JobState::Parked,
            last_error: ErrorClass::TenantNotActive,
            ..j
        },
        (JobState::Waiting, Event::Claim(true)) => {
            // A waiting job at the maximum is not run again: 0005 has no such state.
            if j.attempts < MAX_ATTEMPTS {
                Job {
                    state: JobState::Running,
                    attempts: j.attempts + 1,
                    lease_until: now.saturating_add(LEASE_SECONDS),
                    ..j
                }
            } else {
                Job {
                    state: JobState::Parked,
                    last_error: ErrorClass::JobFailed,
                    ..j
                }
            }
        }
        (JobState::Running, Event::Succeed) => Job {
            state: JobState::Done,
            ended_at: now,
            ..j
        },
        (JobState::Running, Event::Fail) => {
            if j.attempts < MAX_ATTEMPTS {
                Job {
                    state: JobState::Waiting,
                    run_after: now.saturating_add(backoff_seconds(j.attempts)),
                    last_error: ErrorClass::JobFailed,
                    ..j
                }
            } else {
                Job {
                    state: JobState::Parked,
                    last_error: ErrorClass::JobFailed,
                    ..j
                }
            }
        }
        (JobState::Running, Event::LeaseExpired) => {
            if j.attempts < MAX_ATTEMPTS {
                Job {
                    state: JobState::Waiting,
                    run_after: now,
                    last_error: ErrorClass::LeaseExpired,
                    ..j
                }
            } else {
                Job {
                    state: JobState::Parked,
                    last_error: ErrorClass::LeaseExpired,
                    ..j
                }
            }
        }
        (JobState::Parked, Event::Requeue) => Job {
            state: JobState::Waiting,
            attempts: 0,
            run_after: now,
            last_error: ErrorClass::NoError,
            ..j
        },
        _ => j,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting() -> Job {
        Job {
            state: JobState::Waiting,
            attempts: 0,
            run_after: 0,
            lease_until: 0,
            ended_at: 0,
            last_error: ErrorClass::NoError,
        }
    }

    #[test]
    fn claim_sets_lease_and_saturates() {
        let j = job_step(waiting(), Event::Claim(true), 1000);
        assert_eq!(
            (j.state, j.attempts, j.lease_until),
            (JobState::Running, 1, 1300)
        );
        assert_eq!(
            job_step(waiting(), Event::Claim(true), u64::MAX).lease_until,
            u64::MAX
        );
    }

    #[test]
    fn backoff_follows_the_schedule() {
        let mut j = waiting();
        let mut waits = Vec::new();
        for _ in 0..4 {
            j = job_step(j, Event::Claim(true), 0);
            j = job_step(j, Event::Fail, 0);
            waits.push(j.run_after);
        }
        assert_eq!(waits, [30, 120, 600, 1800]);
        j = job_step(job_step(j, Event::Claim(true), 0), Event::Fail, 0);
        assert_eq!((j.state, j.attempts), (JobState::Parked, MAX_ATTEMPTS));
    }
}
