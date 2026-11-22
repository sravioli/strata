# Spec Delta

## ADDED Requirements

### Requirement: Job requeue command

`strata admin job requeue --tenant <name>` SHALL give `Requeue` to `job_step` for each `Parked` job of the tenant, in one statement. Each such job SHALL be `Waiting`, with `attempts` 0 and `run_after` at the time of the command. Jobs in other states and jobs of other tenants SHALL NOT change. The command SHALL print the count of requeued jobs. A tenant that does not exist SHALL give an error and no write.

#### Scenario: TC-5-07 Requeue a parked job

- **GIVEN** a `Parked` job
- **WHEN** the operator requeues it
- **THEN** the job is `Waiting` with `attempts` at zero, and a worker runs it
- **PENDING** Task 7.1 adds the test.

#### Scenario: Only the parked jobs of the tenant

- **GIVEN** a `Parked` job and a `Waiting` job of `acme`, and a `Parked` job of `globex`
- **WHEN** an operator requeues the jobs of `acme`
- **THEN** the `Parked` job of `acme` is `Waiting`, and the other two jobs do not change
- **AND** the command prints 1

#### Scenario: A tenant that does not exist

- **WHEN** an operator requeues the jobs of a tenant that does not exist
- **THEN** the command exits with an error, and no job changes
