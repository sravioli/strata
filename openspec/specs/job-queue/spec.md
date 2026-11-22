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
