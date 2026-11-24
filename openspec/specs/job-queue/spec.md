# job-queue Specification

## Purpose
Keeps the work of the workers in one global PostgreSQL table, `jobs`, that holds only identifiers, a state and an error class (tech design 0005).

## Requirements

### Requirement: The job table holds no tenant content

`jobs` SHALL be a global table with only the fields of 0005: `job_id`, `tenant_id`, `kind`, the target (`document_id` and `revision`), `state`, `attempts`, `run_after`, `lease_until`, `lease_token`, `ended_at` and `last_error`.

#### Scenario: The columns of the table

- **WHEN** a test reads the columns of `jobs` from the catalog
- **THEN** the columns are the fields of the requirement, and no other column exists

### Requirement: Each coded field of a job has a fixed list

`kind` SHALL be `ProcessDocument` or `RemoveFiles`. `state` SHALL be `Waiting`, `Running`, `Parked` or `Done`. `last_error` SHALL be `NoError`, `TenantNotActive`, `LeaseExpired` or `JobFailed`.

#### Scenario: A value that is not in a list

- **WHEN** a transaction writes a `kind`, a `state` or a `last_error` that is not in its list
- **THEN** PostgreSQL rejects the write

### Requirement: The job table has the three indexes of 0005

`jobs` SHALL have an index on the `Waiting` jobs by `run_after`, one on the `Running` jobs by `lease_until`, and one on the `Done` jobs by `ended_at`.

#### Scenario: The indexes exist

- **WHEN** a test reads the indexes of `jobs` from the catalog
- **THEN** each of the three partial indexes exists

### Requirement: The finish creates the job in its transaction

The transaction that sets a revision to `Uploaded` SHALL insert one `ProcessDocument` job for that revision, in the state `Waiting`, with `attempts` 0 and `run_after` at the time of the insert. If the transaction rolls back, no job SHALL exist.

#### Scenario: TC-5-01 Commit and rollback of an upload

- **GIVEN** an upload
- **WHEN** its transaction commits
- **THEN** one `Waiting` job exists for the revision
- **WHEN** its transaction rolls back
- **THEN** no job exists

### Requirement: The API reads only the state of a job

`strata_api` SHALL insert rows of `jobs` and read only the column `state`. Thus it can count the `Waiting` jobs and cannot read the identifiers of a job.

#### Scenario: TC-5-08 The API role reads only the state

- **GIVEN** the database role of the API
- **WHEN** it reads a column of `jobs` other than `state`
- **THEN** the database refuses the read
- **WHEN** it counts the `Waiting` jobs
- **THEN** the count agrees with the table

### Requirement: The cleanup removes old Done jobs

The cleanup of jobs SHALL run as `strata_worker`. It SHALL remove the `Done` jobs whose `ended_at` is more than 24 hours ago, at most 1,000 rows in each batch. It SHALL not remove a job in each other state.

#### Scenario: TC-5-10 A job of 23 hours stays and a job of 25 hours goes

- **GIVEN** a `Done` job that ended 23 hours ago and one that ended 25 hours ago
- **WHEN** the cleanup task runs
- **THEN** the first job stays and the second job is removed

#### Scenario: Jobs in other states stay

- **GIVEN** a `Waiting`, a `Running` and a `Parked` job, each created 25 hours ago
- **WHEN** the cleanup task runs
- **THEN** each job stays

### Requirement: One worker claims one job

A worker SHALL claim one `ProcessDocument` job with `for update skip locked`: a `Waiting` job with `run_after` in the past, or a `Running` job with `lease_until` in the past. The claim SHALL write the result of `job_step` and a new random `lease_token`. A worker SHALL NOT claim a `RemoveFiles` job.

#### Scenario: TC-5-02 Two workers and one job

- **GIVEN** one `Waiting` job and two workers
- **WHEN** the two workers claim at the same time
- **THEN** one worker gets the job, and the other gets no job

#### Scenario: A job that is not due

- **GIVEN** a `Waiting` job with `run_after` in the future, and a `Running` job with `lease_until` in the future
- **WHEN** a worker claims
- **THEN** it gets no job

#### Scenario: A RemoveFiles job

- **GIVEN** a `Waiting` job of kind `RemoveFiles`
- **WHEN** a worker claims
- **THEN** it gets no job, and the job does not change

### Requirement: The lease protects the result of a job

The worker SHALL renew the lease each 100 seconds, for 300 seconds from the renewal. A renewal and the end of a job SHALL write only when `lease_token` is the token of the claim. When a renewal writes nothing, the worker SHALL stop the extraction and write no result.

#### Scenario: TC-5-03 A worker that stops

- **GIVEN** a `Running` job whose worker stops
- **WHEN** the lease expires
- **THEN** a second worker claims the job and ends it, and the index holds the revision one time
- **WHEN** the first worker tries to write its result
- **THEN** the write fails

#### Scenario: A renewal after a new claim

- **GIVEN** a job that a second worker claimed after the lease of the first worker expired
- **WHEN** the first worker renews its lease
- **THEN** the renewal writes nothing, and the first worker stops its extraction

#### Scenario: An expired lease at the maximum of attempts

- **GIVEN** a `Running` job with 5 attempts and an expired lease
- **WHEN** a worker claims
- **THEN** the job is `Parked` with `LeaseExpired`, and the worker gets no job

### Requirement: A failed run waits for its backoff

A normal failure SHALL give `Fail` to `job_step`. Below 5 attempts, the job SHALL be `Waiting` with the backoff of 0005 in `run_after`. At 5 attempts, the job SHALL be `Parked` with `JobFailed`. No worker SHALL claim a `Parked` job.

#### Scenario: TC-5-04 A job that fails at each run

- **GIVEN** a job that fails at each run
- **WHEN** it fails for the maximum count of attempts
- **THEN** the job is `Parked`, `last_error` holds an error class, and no worker claims it

#### Scenario: The backoff after each run

- **GIVEN** a job that fails at each run
- **WHEN** runs 1 to 4 fail
- **THEN** `run_after` is 30 seconds, 2 minutes, 10 minutes and 30 minutes after each failure

### Requirement: A job of a tenant that is not active is parked

When the tenant of a `Waiting` job is not active at the claim, the job SHALL be `Parked` with `TenantNotActive`. `attempts` SHALL NOT change. The worker SHALL NOT start the job.

#### Scenario: TC-5-05 Park and requeue

- **GIVEN** a `Waiting` job of a tenant that is not active
- **WHEN** a worker claims it
- **THEN** the job is `Parked` with `TenantNotActive`, and `attempts` does not change
- **WHEN** the operator sets the tenant to active and requeues the parked jobs of the tenant
- **THEN** the job is `Waiting`, and a worker runs it

### Requirement: A job that ran gives the same state again

A second run of a job whose revision is no longer `Uploaded` SHALL end with `Succeed`, do no extraction and change no revision and no text part.

#### Scenario: TC-5-09 A second run

- **GIVEN** a job that ends with success
- **WHEN** a worker runs the same job a second time
- **THEN** the state of the revision and of its index does not change

### Requirement: The worker reads the object with its own credentials

The worker SHALL read the object of the revision from the storage with its credentials and the storage settings of 0006. It SHALL read at most 100 MB. Each of these SHALL be a normal failure of the job, with no change to the revision: an object that does not exist, an object above 100 MB, and an error of the storage.

#### Scenario: An object that does not exist

- **GIVEN** a `Waiting` job whose object does not exist
- **WHEN** a worker runs it
- **THEN** the revision does not change, and the job is `Waiting` with its backoff

#### Scenario: An object above 100 MB

- **GIVEN** a `Waiting` job whose object is above 100 MB
- **WHEN** a worker runs it
- **THEN** the worker reads at most 100 MB and 1 byte, the revision does not change, and the job is `Waiting` with its backoff

### Requirement: The worker process

`strata worker` SHALL connect as `strata_worker`. It SHALL run at most 2 jobs at the same time. With no job, it SHALL poll after 1 second, two times longer after each empty poll, up to 10 seconds. It SHALL run the cleanup of `Done` jobs each 60 seconds. Its logs SHALL hold no tenant content.

#### Scenario: The cleanup runs in the worker

- **GIVEN** a `Done` job that ended 25 hours ago
- **WHEN** the loop of the worker reaches its first cleanup
- **THEN** the job does not exist

#### Scenario: A setting that is missing

- **WHEN** `strata worker` starts with no `STRATA_NSJAIL`, no `STRATA_EXTRACT` or no `STRATA_CGROUP`, with a path that does not exist, or with a cgroup that it cannot write
- **THEN** it stops with an error that names the variable
