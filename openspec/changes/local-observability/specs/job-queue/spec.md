# Spec Delta

## MODIFIED Requirements

### Requirement: The job table holds no tenant content

`jobs` SHALL be a global table with only the fields of 0005: `job_id`, `tenant_id`, `kind`, the target (`document_id` and `revision`), `state`, `attempts`, `run_after`, `lease_until`, `lease_token`, `ended_at` and `last_error`. It SHALL also have `traceparent`, the trace context of the request that queued the job (0133).

#### Scenario: The columns of the table

- **WHEN** a test reads the columns of `jobs` from the catalog
- **THEN** the columns are the fields of the requirement, and no other column exists

## ADDED Requirements

### Requirement: A job carries the trace context of its request

`traceparent` SHALL be null, or a value of the W3C Trace Context format. That format is version `00`, a trace ID of 32 lowercase hex digits, a span ID of 16 and flags of 2, joined by `-`. PostgreSQL SHALL reject any other value. The finish SHALL write the context of its span. A `RemoveFiles` job SHALL have null. A retry of a job SHALL keep the same value. The worker SHALL run a job with null as before.

#### Scenario: TC-133-05 A value of another format, and null

- **GIVEN** a job row with a `traceparent` that is not in the W3C format
- **WHEN** the row is written
- **THEN** PostgreSQL rejects it
- **GIVEN** a job row with null
- **WHEN** the worker claims it
- **THEN** the job runs, and its span is a root span
- **PENDING** the L1 test comes with the tasks of this change.
