# Spec Delta

## MODIFIED Requirements

### Requirement: The grants are exactly the list

The grants on each table SHALL be exactly these, and no role SHALL have another privilege on them:

- `strata_api`: `select` on `tenants`, `issuers` and `issuer_bindings`. `select` and `insert` on `accounts`. `select`, `insert` and `update` on `documents` and `revisions`. `insert` and `select (state)` on `jobs`. `insert`, `delete` and `select (slot_id, created_at)` on `upload_slots`. `select` on `text_parts`.
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

#### Scenario: The API reads text parts and cannot change them

- **GIVEN** the tenant context of A and a text part of A
- **WHEN** `strata_api` reads the text part, and then updates or removes it
- **THEN** the read gives the part, and PostgreSQL refuses the update and the removal

#### Scenario: The worker cannot change a text part

- **GIVEN** the tenant context of A and a text part of A
- **WHEN** `strata_worker` updates the text of the part
- **THEN** PostgreSQL refuses the update

## ADDED Requirements

### Requirement: Each endpoint is closed to the identifiers of a different tenant

An L2 test SHALL call each endpoint of the public API, over HTTP, with the identifiers of a different tenant. No response SHALL hold data of that tenant, or a count that shows data of that tenant.

#### Scenario: TC-3-19 Each endpoint with the identifiers of B

- **GIVEN** tenants A and B each own data
- **WHEN** an account of A calls each endpoint of the public API with identifiers of B
- **THEN** no response contains data of B or a count that reveals data of B
- **PENDING** Task 7.2 adds the L2 test.
