# Spec Delta

## MODIFIED Requirements

### Requirement: A tenant-owned table follows the three rules

Each tenant-owned table SHALL have a `tenant_id` column that is not null. Its primary key and each unique constraint SHALL start with `tenant_id`. Each foreign key to a tenant-owned parent SHALL include `tenant_id`. The database SHALL generate each identifier with `uuidv7()`.

#### Scenario: TC-3-03 Insert with the tenant of a different tenant fails

- **GIVEN** the tenant context of A
- **WHEN** a transaction inserts a row with the `tenant_id` of B
- **THEN** PostgreSQL rejects the insert

#### Scenario: TC-3-15 Update of the tenant of a row fails

- **GIVEN** the tenant context of A and a row of A
- **WHEN** a transaction updates the `tenant_id` of the row to B
- **THEN** PostgreSQL rejects the update

#### Scenario: Identifiers come from the database

- **GIVEN** the tenant context of A
- **WHEN** a transaction inserts an account with no `account_id`
- **THEN** the row gets a version 7 UUID

#### Scenario: TC-3-04 A child row of A cannot point at a parent row of B

- **GIVEN** the tenant context of A and a document of B
- **WHEN** a transaction inserts a revision of A that points at the document of B
- **THEN** PostgreSQL rejects the insert

### Requirement: Each table has one policy or is a global table

Each tenant-owned table SHALL have row-level security enabled and forced, and exactly one policy, for all commands, with the expression of 0003. `tenants` SHALL have one `select` policy with the expression of 0003. `issuer_bindings` SHALL have one `select` policy with the expression of 0021. Each other table SHALL be in the list of global tables. The list holds `issuers`, `jobs`, `upload_slots` and the history table of the migrations.

#### Scenario: TC-3-06 Each table has its policy or is global

- **WHEN** a test reads each table of the schema from the catalog
- **THEN** the table is in the list of global tables, or it has forced row-level security and exactly one policy
- **AND** the text of that policy is equal to the text in 0003 for a tenant-owned table or for `tenants`
- **AND** for `issuer_bindings`, it is equal to the text in the identity provider design (0021)
- **AND** a tenant-owned table also has a `tenant_id` column

### Requirement: The grants are exactly the list

The grants on each table SHALL be exactly these, and no role SHALL have another privilege on them:

- `strata_api`: `select` on `tenants`, `issuers` and `issuer_bindings`. `select` and `insert` on `accounts`. `select`, `insert` and `update` on `documents` and `revisions`. `insert` and `select (state)` on `jobs`. `insert`, `delete` and `select (slot_id, created_at)` on `upload_slots`.
- `strata_worker`: `select` on `tenants`. `select` and `update (ready_revision)` on `documents`. `select` and `update` on `revisions`. `select`, `update` and `delete` on `jobs`.
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

### Requirement: A tenant transaction opens only from a decision or a scope

The code SHALL open a transaction on a tenant-owned table only from the identity transaction, with an "allow" decision or a scope of `authorize` or `authorized_scope` (0004). Each function that queries a tenant-owned table SHALL take a tenant transaction as its handle. Tests SHALL open a tenant transaction only through the helper that uses the same function.

#### Scenario: A query function takes no pool or connection

- **WHEN** code passes a pool or a plain connection to a function that queries a tenant-owned table
- **THEN** the code does not compile

#### Scenario: A decision of deny opens no tenant transaction

- **GIVEN** an identity transaction and a decision of "deny"
- **WHEN** the code asks for a tenant transaction
- **THEN** it gets none, and the identity transaction rolls back

#### Scenario: TC-4-08 A route gets its tenant transaction only from a decision or a scope

- **GIVEN** each route of the API
- **WHEN** the code of a route opens a tenant transaction in a way other than from a decision or a scope, or runs a query in the identity transaction other than the reads of the binding, the account and the statuses
- **THEN** the code does not compile
- **PENDING** Task 7.2 adds the test.

## ADDED Requirements

### Requirement: Each route has a test with the identifiers of a different tenant

For each route of the API, an L1 test SHALL call the route with the identifiers of a different tenant, and SHALL check that the answer is the answer for identifiers that do not exist. For a route with no identifier in its path, the test SHALL check that a different tenant cannot reach the row that the route creates. A check SHALL fail the build when a route has no such test.

#### Scenario: TC-3-18 Each route has its test

- **WHEN** the check reads each route of the router of `strata api`
- **THEN** each route has a test that uses the identifiers of a different tenant
- **WHEN** a route has no such test
- **THEN** the check fails
- **PENDING** Task 7.1 adds the test.
