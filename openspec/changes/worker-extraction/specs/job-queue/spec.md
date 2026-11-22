# Spec Delta

## ADDED Requirements

### Requirement: One worker claims one job

A worker SHALL claim one `ProcessDocument` job with `for update skip locked`: a `Waiting` job with `run_after` in the past, or a `Running` job with `lease_until` in the past. The claim SHALL write the result of `job_step` and a new random `lease_token`. A worker SHALL NOT claim a `RemoveFiles` job.

#### Scenario: TC-5-02 Two workers and one job

- **GIVEN** one `Waiting` job and two workers
- **WHEN** the two workers claim at the same time
- **THEN** one worker gets the job, and the other gets no job
- **PENDING** Task 5.2 adds the test.

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
- **PENDING** Task 5.5 adds the test.

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
- **PENDING** Task 5.4 adds the test.

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
- **PENDING** Task 7.1 adds the test.

### Requirement: A job that ran gives the same state again

A second run of a job whose revision is no longer `Uploaded` SHALL end with `Succeed`, do no extraction and change no revision and no text part.

#### Scenario: TC-5-09 A second run

- **GIVEN** a job that ends with success
- **WHEN** a worker runs the same job a second time
- **THEN** the state of the revision and of its index does not change
- **PENDING** Task 5.4 adds the test.

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

- **WHEN** `strata worker` starts with no `STRATA_NSJAIL`, no `STRATA_EXTRACT`, or a path that does not exist
- **THEN** it stops with an error that names the variable
