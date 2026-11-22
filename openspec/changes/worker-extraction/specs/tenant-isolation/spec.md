# Spec Delta

## MODIFIED Requirements

### Requirement: The grants are exactly the list

The grants on each table SHALL be exactly these, and no role SHALL have another privilege on them:

- `strata_api`: `select` on `tenants`, `issuers` and `issuer_bindings`. `select` and `insert` on `accounts`. `select`, `insert` and `update` on `documents` and `revisions`. `insert` and `select (state)` on `jobs`. `insert`, `delete` and `select (slot_id, created_at)` on `upload_slots`.
- `strata_worker`: `select` on `tenants`. `select` and `update (ready_revision)` on `documents`. `select` and `update` on `revisions`. `select`, `insert` and `delete` on `text_parts`. `select`, `update` and `delete` on `jobs`.
- `strata_operator`: `select`, `insert` and `update` on `tenants` and `issuers`. `select`, `insert` and `delete` on `issuer_bindings`. `select` and `update` on `accounts`. `select` and `update` on `jobs`.

#### Scenario: No privilege beyond the list

- **WHEN** a test reads each table privilege and each column privilege of `strata_api`, `strata_worker` and `strata_operator` from the catalog
- **THEN** each privilege is in the list of the requirement, and each privilege of the list exists

#### Scenario: The API cannot change the status of an account

- **GIVEN** the tenant context of A and an account of A
- **WHEN** `strata_api` updates the status of the account
- **THEN** PostgreSQL refuses the update

#### Scenario: The API removes a row of upload_slots by its key only

- **GIVEN** a row of `upload_slots`
- **WHEN** `strata_api` removes the row by its `slot_id`
- **THEN** the row is removed
- **WHEN** `strata_api` reads a column of `upload_slots` other than `slot_id` and `created_at`
- **THEN** PostgreSQL refuses the read

#### Scenario: The worker cannot change a text part

- **GIVEN** the tenant context of A and a text part of A
- **WHEN** `strata_worker` updates the text of the part
- **THEN** PostgreSQL refuses the update

## ADDED Requirements

### Requirement: A job reads only the rows of its tenant

The worker SHALL read and write the rows of a job only in the tenant transaction of the tenant of the job. A job whose revision the worker does not find SHALL end with `Succeed`, with no extraction and no write.

#### Scenario: TC-3-17 A job that names a document of B

- **GIVEN** a job of tenant A whose content names a document of B
- **WHEN** a worker runs the job
- **THEN** the worker finds no document and reads no row of B
- **PENDING** Task 5.4 adds the test.

#### Scenario: TC-4-07 The decision for a job

- **GIVEN** a job of tenant A
- **WHEN** the worker processes a document of A
- **THEN** the decision is "allow"
- **WHEN** the content of the job names a document of B
- **THEN** the decision is "deny"
- **PENDING** Task 5.4 adds the test.

### Requirement: The tenant status applies at the next claim

A job that runs when its tenant becomes not active SHALL continue to its end. The next claim of a job of that tenant SHALL get "deny", and the worker SHALL NOT start that job.

#### Scenario: TC-4-10 A tenant stops while a job runs

- **GIVEN** a job of tenant A that runs
- **WHEN** an operator sets A to not active
- **THEN** the job continues to its end
- **WHEN** a worker claims the next job of A
- **THEN** the decision is "deny" and the worker does not start the job
