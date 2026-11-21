---
status: "accepted"
date: 2026-11-18
decision-makers: sravioli
tech-design: docs/tech-design/0005-job-queue.md
---

# A job table in PostgreSQL owns the work, and a message bus only wakes workers

## Context and Problem Statement

An upload creates work that a worker does later. Strata needs a queue that never loses a job, never runs one job on two workers at the same time, and keeps the tenant boundary. Where does the work live, and what happens to a job that fails or whose tenant is not active? The issue is [#5](https://github.com/sravioli/strata/issues/5).

## Considered Options

* A job table in PostgreSQL, with an optional bus that only wakes workers
* A transactional outbox and Azure Service Bus
* A job table, with a message bus in v1
* A job table, with PostgreSQL `LISTEN` and `NOTIFY` in v1

## Decision Outcome

Chosen option: "A job table in PostgreSQL, with an optional bus that only wakes workers", because a job commits with its cause. A lost or repeated message then changes no result.

The decision has these parts:

* One global table, `jobs`, holds identifiers, state and an error class. It holds no tenant content.
* The upload transaction creates the revision and its job together.
* A worker claims a job with `FOR UPDATE SKIP LOCKED` and gets a lease with a token. When the lease expires, a different worker can claim the job. The claim query also takes `Running` jobs whose lease expired (Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85))). The write of the first worker then fails.
* A job can run more than one time. Each kind of job must give the same result for a second run.
* A job that fails 5 times becomes `Parked`. The backoff is 30 seconds, 2 minutes, 10 minutes and 30 minutes.
* A job of a tenant that is not active becomes `Parked` at the claim, with no change to its count of attempts. An operator requeues it.
* A job that ends with success becomes `Done`. A cleanup task removes `Done` jobs after 24 hours.
* The count of `Waiting` jobs has a bound. At the bound, an upload gets 503 with `Retry-After`. The bound is approximate (Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85))).
* v1 has no message bus. Workers ask for work at an interval. A later version must be able to add a bus that wakes workers, with no change to the job table.
* The API can insert jobs and count `Waiting` jobs. It can read only the column `state` (Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85))).
* A pure function, `job_step`, gives each change of state. Lean proves six statements about it.

### Consequences

* Good, because no job exists without the change that causes it, and no change exists without its job.
* Good, because Strata needs no outbox, dispatcher or inbox table.
* Good, because a bus is an addition that cannot break correctness, so Strata can support more than one bus.
* Good, because L1 tests need only PostgreSQL.
* Bad, because the queue adds writes to the primary database.
* Bad, because parked jobs need their own view and operator command. There are no ready-made tools for dead letters.
* Bad, because the signal to add workers comes from a query, not from a broker.
* Bad, because a worker can find a job up to approximately one second late. The target of 5 minutes permits this.
* Not decided: which bus, and when Strata adds it. The steps inside a job ([#7](https://github.com/sravioli/strata/issues/7)).
