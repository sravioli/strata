- Issue: [#5](https://github.com/sravioli/strata/issues/5)
- Status: draft
- Start date: 2026-11-18
- Decided by: (not decided)

# Job queue: PostgreSQL owns the work, buses are optional

## Problem description

An upload creates work that a worker does later: extract the text of a revision and index it. Strata needs a queue for this work. The queue must never lose a job, never run one job on two workers at the same time, and keep the tenant boundary. This design puts the queue in PostgreSQL, in the same database as the state that causes each job.

Goals:

- One source of truth for work, in the same transaction as the change that causes the work.
- A worker that stops does not lose its job.
- A job that fails many times stops and waits for an operator.
- A decision for a job of a tenant that is not active (issue [#4](https://github.com/sravioli/strata/issues/4)).

Non-goals:

- The steps inside a job: extraction, chunking, embedding. Issue [#7](https://github.com/sravioli/strata/issues/7) and milestone M2 decide them.
- How a client learns that a document is ready (M1).
- A message bus in v1.

## Background and references

The handoff package proposed four mechanisms: a transactional outbox, a dispatcher, Azure Service Bus, and an inbox table to remove duplicate messages (`docs/adr/draft/ADR-0004-reliable-ingestion-and-idempotency.md`). All four exist because the queue is outside the database. The owner said that Service Bus is not a constraint, and that Strata can support more than one bus (map [#1](https://github.com/sravioli/strata/issues/1)).

The tenancy design ([#3](https://github.com/sravioli/strata/issues/3), ADR 0001) names the job queue as a global table. A global table has no row-level security and must not hold tenant content. It can hold identifiers and state. A worker claims a job, reads its `tenant_id`, then opens a tenant transaction for that tenant.

The v1 authorization design (ADR 0002) makes a job a principal of one tenant. A worker reads the tenant status when it claims a job. If the tenant is not active, the decision is "deny" and the worker does not start the job. ADR 0002 left one question to this design: does a denied job wait or fail?

The capacity design (ADR 0003) sets the ceiling at 10,000 uploads each hour, approximately 3 each second. It requires a bound on the queue, and a 503 response with `Retry-After` when the queue is full. 95 % of uploaded documents must become searchable in 5 minutes or less.

Spike [#8](https://github.com/sravioli/strata/issues/8) proved that the state machine of a revision ignores a repeated event. Thus a job that runs two times gives the same result as a job that runs one time.

References:

- PostgreSQL documentation, "The Locking Clause" (`FOR UPDATE SKIP LOCKED`): https://www.postgresql.org/docs/current/sql-select.html#SQL-FOR-UPDATE-SHARE
- PostgreSQL documentation, "Transaction Isolation": https://www.postgresql.org/docs/current/transaction-iso.html

## Requirements

| ID | Requirement |
|---|---|
| Q1 | A job exists exactly when the transaction that causes it commits. |
| Q2 | Two workers never run the same job at the same time, except after the lease of the first worker expires. |
| Q3 | A worker that stops does not lose its job. Another worker claims the job when the lease expires. |
| Q4 | A job runs at least one time. A job can run more than one time, and each run gives the same result. |
| Q5 | A job that fails a fixed number of times stops, and an operator decides what to do. |
| Q6 | A worker does not start a job of a tenant that is not active. |
| Q7 | The job table holds no tenant content. |
| Q8 | The count of waiting jobs has a bound. At the bound, an upload gets 503 with `Retry-After`. |
| Q9 | The correctness of the queue does not depend on a message bus. |
| Q10 | A later version can add a message bus that wakes workers, with no change to the job table or to the job state function. |
| Q11 | A `Done` job stays in the table for 24 hours, then the cleanup task removes it. |

Quantities, from ADR 0003:

- 10,000 new jobs each hour at the ceiling. That is approximately 3 each second.
- Each job changes its row approximately three times: claim, lease renewal, end. That is approximately 10 writes each second on the primary.
- 95 % of jobs end 5 minutes or less after the upload. That includes the time that the job waits.

## Proposed design

### The job table

One global table, `jobs`. Each row is one job. A row holds only these fields:

| Field | Meaning |
|---|---|
| `job_id` | The identifier of the job |
| `tenant_id` | The tenant that the job is for |
| `kind` | The kind of job. v1 has one kind: `ProcessDocument`. A later design adds kinds. |
| `subject` | The identifiers that the job works on, for example the document and the revision number |
| `state` | `Waiting`, `Running`, `Parked` or `Done` |
| `attempts` | The count of runs that started |
| `run_after` | The earliest time that a worker can claim the job |
| `lease_until` | For a `Running` job: the time when the lease expires |
| `lease_token` | For a `Running` job: a new random value at each claim. Only the worker that has this value can renew the lease or end the job. |
| `ended_at` | For a `Done` job: the time when it ended |
| `last_error` | An error class from a fixed list, for example `ExtractionTimeout`. Never an error text. |

`last_error` is a class, not a text, because an error text can contain tenant content, for example a line of a document. The worker writes the full error to the log of the tenant transaction. Issue [#7](https://github.com/sravioli/strata/issues/7) decides where extraction errors go.

A job that ends with success becomes `Done` in the transaction that writes its result. A `Done` job stays in the table for 24 hours. A cleanup task then removes `Done` jobs that are older than 24 hours, in batches. Thus an operator can see when a revision was processed, and the table stays small: approximately 240,000 `Done` rows at the ceiling.

### Life of a job

```
            upload commits
                  |
                  v
   +-------> Waiting <------------------------------+
   |              |                                 |
   |   claim, tenant active        claim, tenant not active
   |              |                                 |
   |              v                                 v
   |          Running                     Parked (TenantNotActive)
   |          /   |   \
   |  fail,      ok    lease expires
   |  attempts    |        |
   |  < max       |        +--> Waiting
   |     |        v
   +-----+       Done --> (removed after 24 hours)
              
   Running -- fail or lease expires, attempts = max --> Parked
   Parked -- operator requeues --> Waiting
```

1. **Create.** The upload transaction inserts the revision and its job together. If the transaction rolls back, no job exists (Q1).
2. **Claim.** A worker selects one `Waiting` job with `run_after` in the past, with `FOR UPDATE SKIP LOCKED`. Two workers never get the same row (Q2). In the same transaction, the worker reads the tenant status and gives the decision to the job state function.
3. **Run.** If the tenant is active, the job becomes `Running`, `attempts` increases by one, and `lease_until` is set. The worker opens a tenant transaction for the tenant of the job and does the work. While it works, it renews the lease at intervals.
4. **End with success.** The worker writes the result and sets the job to `Done` in one transaction.
5. **End with failure.** If `attempts` is less than the maximum, the job becomes `Waiting` with a later `run_after` (a backoff). If `attempts` is at the maximum, the job becomes `Parked` with its error class (Q5).
6. **Lease expires.** A worker that stops renews no lease. Another worker can then claim the job (Q3). If `attempts` is at the maximum when the lease expires, the job becomes `Parked` with the error class `LeaseExpired`. The first worker, if it still runs, cannot write its result: the write checks the `lease_token`, and the new claim changed it.

### A job of a tenant that is not active

When the tenant is not active at the claim, the job becomes `Parked` with the error class `TenantNotActive`. `attempts` does not change. No worker claims a `Parked` job.

After the operator sets the tenant to active again, the operator requeues the parked jobs of that tenant. The operator command can requeue each parked job of one tenant with one call.

A job that already runs when its tenant becomes not active continues to its end (ADR 0002).

This answers the question of ADR 0002: the job stops and waits for an operator. The owner decided this on 2026-11-18.

### Duplicate runs

A job can run more than one time. Example: a worker is slow, its lease expires, and a second worker runs the same job. The write of the first worker then fails, but its work outside the database, for example a file in object storage, can happen two times. Each kind of job must give the same result for a second run. For `ProcessDocument`, the revision state machine of spike [#8](https://github.com/sravioli/strata/issues/8) gives this property. A repeated event leaves the state unchanged, and an older revision never replaces a newer active one (Q4).

Strata does not need an inbox table, because the job and its result are in the same database.

### How a worker finds work

In v1 each worker asks the database for a job at an interval, for example each second. When it finds no job, it waits longer, up to a maximum. At the ceiling of ADR 0003 this costs one small query each second for each worker.

v1 has no message bus. The owner decided on 2026-11-18 that a later version must be able to add one (Q10). The worker has one point where it waits for work. In v1 a timer ends the wait. A later version can also end the wait when a message comes from a bus, or from PostgreSQL `LISTEN` and `NOTIFY`. A message only makes a worker ask sooner. The job table stays the only source of truth, so a lost or repeated message changes no result (Q9). Thus Strata can support more than one bus, and a bus adds no correctness risk.

The API sends a message after the upload transaction commits. If the send fails, the timer finds the job. The design for clients that must learn that a document is ready (M1) is a good place to choose the bus.

### Bounds

- The count of `Waiting` jobs has a bound that the configuration sets. An upload that finds the count at the bound gets 503 with `Retry-After`, and its transaction creates no revision and no job (Q8).
- The count of jobs that one worker runs at the same time has a bound.
- The maximum of `attempts` and the backoff are fixed values in the configuration.

### Who can read and change the table

The tenancy design gives each process its own database role. This design adds these rights on `jobs`:

| Role | Rights on `jobs` |
|---|---|
| API | Insert, and a count of `Waiting` jobs. No read of rows. |
| Worker | Read, update and remove. |
| Operator command | Read, update. It lists `Parked` jobs and requeues them. |

The API cannot list the jobs of other tenants. A worker can read each row, but a row holds only identifiers and state (Q7).

### The job state function

A pure function in the core crate, `job_step(job, event, now)`, gives the next state of a job. The SQL of each step writes the result of this function. The events are: `Claim(tenant_active)`, `Succeed`, `Fail`, `LeaseExpired` and `Requeue`. An event that does not apply leaves the job unchanged.

## Alternatives considered

**An outbox and Azure Service Bus, as in the handoff.** The bus gives ready-made dead-letter tools and a signal for autoscaling. But the job then exists in two places, and Strata needs an outbox, a dispatcher and an inbox to keep the two in agreement. Each mechanism is more code to test, and the only Rust crate for Service Bus is not official and had no release since October 2024. With the job in PostgreSQL, a bus can come later as a signal only.

**A job of a tenant that is not active waits in a `Held` state.** The operator command that sets the tenant to active then releases the jobs, with no step by hand. But it adds a state and a rule to the operator command. The owner chose `Parked` on 2026-11-18, so that an operator decides about each job.

**A job of a tenant that is not active waits with a later `run_after`.** This needs no new state. But a worker then claims and releases the job at each interval for as long as the tenant is not active. That is work with no result.

**Remove a job at once when it ends with success.** Graphile Worker and Que do this. The table is then smallest. But an operator must then search the log to see when a revision was processed. Many queue libraries on PostgreSQL, for example River and GoodJob, keep ended jobs for a period. The owner chose 24 hours on 2026-11-18.

**A message bus in v1.** A bus wakes a worker in milliseconds, not in approximately one second. But it adds a service, a test container and its cost, and the Rust crate for Azure Service Bus is not official. The target of 5 minutes does not need the faster wake. The owner decided that v1 has no bus, and that a later version must be able to add one.

**`LISTEN` and `NOTIFY` in v1.** This wakes a worker at once. But `LISTEN` needs a session connection, and the pooler of ADR 0001 is in transaction mode. A worker then needs a second, direct connection. At 3 jobs each second, an interval of one second is enough to meet the 5-minute target.

## Proof obligations

Each theorem is about the Rust function `job_step` in the core crate. It uses enums, structs, `match` and integer comparison, which spike [#8](https://github.com/sravioli/strata/issues/8) showed that Aeneas translates.

| Theorem | Requirement it covers | Plain-English statement | Lean statement | Rust function it is about |
|---|---|---|---|---|
| `inactive_never_runs` | Q6 | A claim of a `Waiting` job when the tenant is not active never makes the job `Running`. | `j.state = .waiting → (jobStep j (.claim false) now).state ≠ .running` | `job_step` |
| `inactive_parks` | Q6 | A claim of a `Waiting` job when the tenant is not active makes the job `Parked`, and it does not change the count of attempts. | `j.state = .waiting → jobStep j (.claim false) now = { j with state := .parked, lastError := .tenantNotActive }` | `job_step` |
| `attempts_bounded` | Q5 | If the count of attempts is at most the maximum before a step, it is at most the maximum after the step. | `j.attempts ≤ max → (jobStep j e now).attempts ≤ max` | `job_step` |
| `fail_at_max_parks` | Q5 | A failure or an expired lease when the count of attempts is at the maximum makes the job `Parked`. | `j.state = .running → j.attempts = max → e ∈ [.fail, .leaseExpired] → (jobStep j e now).state = .parked` | `job_step` |
| `done_final` | Q11 | No event changes a `Done` job. | `j.state = .done → jobStep j e now = j` | `job_step` |
| `parked_needs_operator` | Q5 | Only `Requeue` takes a job out of `Parked`. | `j.state = .parked → e ≠ .requeue → (jobStep j e now).state = .parked` | `job_step` |

Assumptions:

- PostgreSQL transactions are atomic, and `FOR UPDATE SKIP LOCKED` gives each row to one transaction. The proofs do not show this. Test cases TC-5-01 and TC-5-02 cover it.
- The SQL of each step writes the result of `job_step`. The proofs do not show this. L1 test cases cover each step.
- A second run of a job gives the same result. Spike [#8](https://github.com/sravioli/strata/issues/8) proved this for the revision state machine. The design for each new kind of job must show it again.
- Issue [#9](https://github.com/sravioli/strata/issues/9) decides the proof route. The statements stay the same on each route.

## Impacts

### Infrastructure

- No new service. The queue uses the primary database. At the ceiling, it adds approximately 3 inserts and 10 updates each second.
- Metrics: the count of jobs in each state, and the age of the oldest `Waiting` job.
- Metrics: the count of runs and failures for each kind, and the count of expired leases.
- Alerts: a `Parked` job, and an oldest `Waiting` job that is older than 5 minutes.
- The age of the oldest `Waiting` job is the signal to add or remove workers.
- Rollout: this design is part of the first version. There is no earlier state.

### Security and tenant isolation

- A job row holds identifiers, state and an error class. It holds no tenant content (Q7).
- A worker opens a tenant transaction for the tenant of the job. Row-level security then limits each read and write to that tenant. A job that names a document of a different tenant gets "deny" from `authorize` (ADR 0002, TC-4-07), and row-level security also finds no row.
- The API can insert a job and count `Waiting` jobs. It cannot read rows, so it cannot list the identifiers of other tenants.
- A compromised worker can read the identifiers of each job. It can also open a tenant transaction for each tenant. This is the same risk that ADR 0001 states for a worker.
- New input that is not trusted: none. The API creates each job from the upload that it already checked.

### Data and migration

- A new global table, `jobs`, with an index on the `Waiting` jobs by `run_after`, and an index on the `Done` jobs by `ended_at` for the cleanup task.
- No change to tenant-owned tables.
- The job table is part of the first schema. There is no earlier version.

### Developer

- Each new kind of job needs a value of `kind`, a second run that gives the same result, and test cases for both.
- L1 tests need PostgreSQL in a container, as for the tenancy design. No bus is necessary for tests.

## Test cases

L0: unit tests cover each row of `job_step` for each state and each event.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-5-01 | L1 | Given an upload. When its transaction commits. Then one `Waiting` job exists for the revision. When its transaction rolls back. Then no job exists. | (not decided) | pending |
| TC-5-02 | L1 | Given one `Waiting` job and two workers. When the two workers claim at the same time. Then one worker gets the job, and the other gets no job. | (not decided) | pending |
| TC-5-03 | L1 | Given a `Running` job whose worker stops. When the lease expires. Then a second worker claims the job and ends it. The index holds the revision one time. When the first worker tries to write its result. Then the write fails. | (not decided) | pending |
| TC-5-04 | L1 | Given a job that fails at each run. When it fails for the maximum count of attempts. Then the job is `Parked`, `last_error` holds an error class, and no worker claims it. | (not decided) | pending |
| TC-5-05 | L1 | Given a `Waiting` job of a tenant that is not active. When a worker claims it. Then the job is `Parked` with `TenantNotActive`, and `attempts` does not change. When the operator sets the tenant to active and requeues the parked jobs of the tenant. Then the job is `Waiting`, and a worker runs it. | (not decided) | pending |
| TC-5-06 | L1 | Given a count of `Waiting` jobs at the bound. When a client uploads a document. Then the API answers 503 with `Retry-After`, and no revision and no job exist for the upload. | (not decided) | pending |
| TC-5-07 | L1 | Given a `Parked` job. When the operator requeues it. Then the job is `Waiting` with `attempts` at zero, and a worker runs it. | (not decided) | pending |
| TC-5-08 | L1 | Given the database role of the API. When it tries to read rows of `jobs`. Then the database refuses the read. | (not decided) | pending |
| TC-5-09 | L1 | Given a job that ends with success. When a worker runs the same job a second time. Then the state of the revision and of its index does not change. | (not decided) | pending |
| TC-5-10 | L1 | Given a `Done` job that ended 23 hours ago and one that ended 25 hours ago. When the cleanup task runs. Then the first job stays and the second job is removed. | (not decided) | pending |
| TC-5-11 | L1 | Given a worker that waits for work, and a source of messages that loses some messages and repeats others. When clients upload documents. Then each job runs, and the result is the same as with no messages. | (not decided) | pending |

The L3 target for the queue is TC-2-04 of ADR 0003.

## Unresolved questions

The owner answered on 2026-11-18:

- A job of a tenant that is not active becomes `Parked`.
- The maximum of attempts is 5 runs, with a backoff of 30 seconds, 2 minutes, 10 minutes and 30 minutes.
- A `Done` job stays for 24 hours.
- v1 has no message bus. A later version must be able to add one.

Implementation resolves these points:

- The length of a lease, the interval of lease renewal, and the interval at which a worker asks for work.
- The value of the bound on `Waiting` jobs, and how the API counts them.
- The fixed list of error classes.
- The interval of the cleanup task and the size of its batches.

These questions belong to later designs:

- The steps inside `ProcessDocument`, and where extraction errors go ([#7](https://github.com/sravioli/strata/issues/7)).
- Which bus wakes workers, and when Strata adds it.
- New kinds of jobs, for example the removal of stored files ([#6](https://github.com/sravioli/strata/issues/6)).
