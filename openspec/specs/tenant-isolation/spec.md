# tenant-isolation Specification

## Purpose
Keeps the rows of each tenant separate in the shared schema of ADR 0001. The application, row-level security and the constraints of PostgreSQL each enforce the separation (tech design 0003).

## Requirements

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

### Requirement: Each table has one policy or is a global table

Each tenant-owned table SHALL have row-level security enabled and forced, and exactly one policy, for all commands, with the expression of 0003. `tenants` SHALL have one `select` policy with the expression of 0003. `issuer_bindings` SHALL have one `select` policy with the expression of 0021. Each other table SHALL be in the list of global tables. In this change the list holds `issuers` and the history table of the migrations.

#### Scenario: TC-3-06 Each table has its policy or is global

- **WHEN** a test reads each table of the schema from the catalog
- **THEN** the table is in the list of global tables, or it has forced row-level security and exactly one policy
- **AND** the text of that policy is equal to the text in 0003 for a tenant-owned table or for `tenants`
- **AND** for `issuer_bindings`, it is equal to the text in the identity provider design (0021)
- **AND** a tenant-owned table also has a `tenant_id` column

### Requirement: A transaction sees only the rows of its tenant context

A runtime role SHALL read and write only rows whose `tenant_id` is the value of `app.tenant_id` in the current transaction. With no tenant context, a runtime role SHALL read no row of a tenant-owned table or of `tenants`, and each write SHALL fail.

#### Scenario: TC-3-01 Select with no filter gives only the rows of the tenant

- **GIVEN** tenants A and B each own one row
- **WHEN** a transaction in the tenant context of A selects all rows with no tenant filter
- **THEN** the result contains only the row of A

#### Scenario: TC-3-02 No tenant context gives no rows and no writes

- **GIVEN** no tenant context
- **WHEN** a runtime role selects from a tenant-owned table
- **THEN** the result is empty
- **WHEN** the role inserts a row
- **THEN** PostgreSQL rejects the insert

#### Scenario: TC-3-20 The tenants table shows only the tenant of the context

- **GIVEN** tenants A and B
- **WHEN** a runtime role selects all rows of `tenants` in the tenant context of A
- **THEN** the result contains only the row of A
- **WHEN** the role selects with no tenant context
- **THEN** the result is empty

### Requirement: The tenant context does not stay on a pooled connection

The code SHALL set `app.tenant_id`, `app.issuer_id` and `app.tenant_value` only with `set_config(…, true)`, so each value applies to one transaction. The prepared statements of the driver SHALL work through PgBouncer in transaction mode.

#### Scenario: TC-3-05 The next transaction on a pooled connection reads only its own tenant

- **GIVEN** a pooler in transaction mode with one server connection
- **WHEN** a transaction of A ends and a transaction of B starts on that connection
- **THEN** the transaction of B reads only rows of B

#### Scenario: Prepared statements work through the pooler

- **GIVEN** PgBouncer in transaction mode with `max_prepared_statements` above 0
- **WHEN** two clients run the same prepared statement in turn, each in its own transaction
- **THEN** each statement gives the result of its own transaction, with no error

### Requirement: The runtime roles have limited rights

`strata_api` and `strata_worker` SHALL not be superusers and SHALL not bypass row-level security. They SHALL not create roles or objects, own a table, truncate a table, or be a member of a role. Only `strata_operator` and the superuser SHALL bypass row-level security.

#### Scenario: TC-3-07 Each runtime role has only the limited rights

- **WHEN** a test reads the attributes, the memberships and the privileges of each runtime role from the catalog
- **THEN** the role is not a superuser and cannot bypass row-level security
- **AND** the role cannot create roles or objects in the schema
- **AND** the role owns no table, cannot truncate a table and is a member of no other role

### Requirement: The grants are exactly the list

The grants on the tables of this change SHALL be exactly these, and no role SHALL have another privilege on them:

- `strata_api`: `select` on `tenants`, `issuers` and `issuer_bindings`. `select` and `insert` on `accounts`.
- `strata_worker`: `select` on `tenants`.
- `strata_operator`: `select`, `insert` and `update` on `tenants` and `issuers`. `select`, `insert` and `delete` on `issuer_bindings`. `select` and `update` on `accounts`.

#### Scenario: No privilege beyond the list

- **WHEN** a test reads each table privilege of `strata_api`, `strata_worker` and `strata_operator` from the catalog
- **THEN** each privilege is in the list of the requirement, and each privilege of the list exists

#### Scenario: The API cannot change the status of an account

- **GIVEN** the tenant context of A and an account of A
- **WHEN** `strata_api` updates the status of the account
- **THEN** PostgreSQL refuses the update

### Requirement: A tenant transaction opens only from a decision or a scope

The code SHALL open a transaction on a tenant-owned table only from the identity transaction, with an "allow" decision or a scope of `authorize` or `authorized_scope` (0004). Each function that queries a tenant-owned table SHALL take a tenant transaction as its handle. Tests SHALL open a tenant transaction only through the helper that uses the same function.

#### Scenario: A query function takes no pool or connection

- **WHEN** code passes a pool or a plain connection to a function that queries a tenant-owned table
- **THEN** the code does not compile

#### Scenario: A decision of deny opens no tenant transaction

- **GIVEN** an identity transaction and a decision of "deny"
- **WHEN** the code asks for a tenant transaction
- **THEN** it gets none, and the identity transaction rolls back
