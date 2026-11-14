- Issue: #3
- Status: draft
- Start date: 2026-11-14
- Decided by: (not decided)

# Tenancy model

## Problem description

Strata keeps the documents of many organisations in one system. An account must read and change only the data of its own tenant. The number of tenants is not known. The total is approximately 40,000 accounts.

Goals:

- One model that works for any number of tenants and any tenant size.
- A tenant boundary that more than one layer enforces.
- A model small enough to describe in Lean.

Non-goals:

- Authorization inside a tenant. Issue #4 owns that decision.
- The identity provider. Milestone M1 owns that decision.
- Dedicated infrastructure for one tenant.
- The layout of the vector index. Milestone M2 owns that decision.
- The removal of a tenant and its data.

## Background and references

The owner made two decisions on 2026-11-14:

- A **tenant** is an organisation. It is a group of several people.
- An **account** belongs to exactly one tenant. A person who works with two organisations has one account in each.

The draft `docs/adr/draft/ADR-0002-multi-tenant-isolation.md` proposed a pooled model. This design keeps that direction and makes it specific.

References:

- PostgreSQL documentation, "Row Security Policies": https://www.postgresql.org/docs/current/ddl-rowsecurity.html
- PostgreSQL documentation, "Configuration Settings Functions" (`set_config`, `current_setting`): https://www.postgresql.org/docs/current/functions-admin.html
- PgBouncer documentation, "Features" (pooling modes): https://www.pgbouncer.org/features.html

Terms used in this design:

- **Tenant-owned table**: a table whose rows each belong to one tenant.
- **Global table**: a table whose rows belong to no tenant.
- **Tenant context**: the tenant that a database transaction acts for.
- **Runtime role**: a database role that the API or a worker uses.
- **Owner role**: the database role that owns the tables and applies migrations.

## Requirements

| ID | Requirement |
|---|---|
| R1 | A transaction reads and changes tenant-owned rows of one tenant only. |
| R2 | The tenant of a request comes from the authenticated account. It never comes from request input. |
| R3 | A transaction with no tenant context gets no tenant-owned rows. |
| R4 | A query that omits the tenant filter because of a programming error exposes no rows of other tenants. |
| R5 | A new tenant costs one row. The design creates no schema, table, partition, index or role for a tenant. |
| R6 | The model works through a connection pooler in transaction mode. |
| R7 | A worker processes jobs of all tenants. Each job runs in the tenant context of exactly one tenant. |

Quantities:

- Accounts: approximately 40,000.
- Tenants: not known. The design must hold from 1 tenant to 40,000 tenants.
- Tenant size: not known. The design must hold when one tenant owns most of the data.

## Proposed design

### Data model

Three tables carry the model:

- `tenants(tenant_id, name, status, created_at)` is tenant-owned.
- `accounts(tenant_id, account_id, status, created_at)` is tenant-owned.
- `login_identities(login_identity, tenant_id, account_id)` is global.

Three rules apply to every tenant-owned table:

1. The table has a `tenant_id` column that is not null.
2. The primary key starts with `tenant_id`. An example is `(tenant_id, document_id)`.
3. A reference to a parent row uses a composite foreign key that includes `tenant_id`. An example is `(tenant_id, document_id)`.

Rule 3 makes a reference to a row of a different tenant impossible. Rule 2 puts `tenant_id` first in every index, which keeps the rows of one tenant together.

Every unique constraint on a tenant-owned table starts with `tenant_id`. A unique constraint that spans tenants reveals that a value exists in a different tenant.

All identifiers are UUIDs that the database generates. A client never supplies an identifier for a new row.

### Three layers of enforcement

**Layer 1: the application.** The code reaches tenant-owned tables only through a tenant transaction. The function that opens a tenant transaction does two things:

1. It starts a database transaction.
2. It runs `select set_config('app.tenant_id', $1, true)`. The value applies to that transaction only.

Functions that query tenant-owned tables accept only a tenant transaction as their database handle. The Rust type system rejects a call that passes a different handle. Each query also contains the filter `tenant_id = $1`.

**Layer 2: row-level security.** Each tenant-owned table has row-level security enabled and forced. Each table has one policy for all commands:

```sql
using      (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
with check (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
```

With no tenant context the expression is null. PostgreSQL then returns no rows and rejects each write. The `nullif` call is necessary. After a transaction sets the value, PostgreSQL gives an empty string, not null, to later transactions on that connection.

**Layer 3: constraints.** The composite foreign keys reject a child row that points at a parent row of a different tenant.

### Database roles

| Role | Used by | Rights |
|---|---|---|
| `strata_owner` | The migration job only | Owns all tables. Applies migrations. |
| `strata_api` | The API | Reads and writes rows. Owns no table. Is not a superuser. Cannot bypass row-level security. Cannot truncate. |
| `strata_worker` | Workers | The same limits as `strata_api`. |

The policies also apply to `strata_owner`, because each table forces row-level security.

### Global tables and the login step

A global table has no row-level security. A global table must not hold tenant content. It can hold identifiers and state.

The login step must find an account before a tenant context exists. The global table `login_identities` does that step:

- Its primary key is `login_identity`. Thus one login identity gives exactly one account.
- A composite foreign key `(tenant_id, account_id)` points at `accounts`.
- It holds no other column.

No database function runs with the rights of the owner role. No role can bypass row-level security.

The job queue of issue #5 is a second example of a global table. A worker claims a job, reads its `tenant_id`, then opens a tenant transaction for that tenant.

### Request flow

1. The API authenticates the caller and gets a login identity.
2. The API reads `login_identities` and gets the tenant and the account.
3. The API opens a tenant transaction for that tenant.
4. The API reads the status of the account and of the tenant. It rejects the request if one of them is not active.
5. The API runs its queries in that transaction.

The request carries no tenant identifier. Because an account belongs to exactly one tenant, the account gives the tenant.

### Connection pooler

The tenant context applies to one transaction. A pooler in transaction mode gives a connection to one transaction at a time. Thus a later transaction on the same connection does not get the earlier tenant context.

### Isolation tiers

This design has one tier: all tenants share one deployment. A tenant that needs stronger isolation gets a separate deployment of the complete system. This design adds no code for that case.

## Alternatives considered

**One schema or one database for each tenant.** The boundary is strong. But each migration runs one time for each tenant, and each tenant needs its own connections. The tenant count is not known, so this fails R5.

**Shared schema with application filters only.** This is the simplest model. One query with no tenant filter exposes all tenants. This fails R4.

**Row-level security with no filter in the queries.** This meets R1 to R4. The design keeps the explicit filter for two reasons. The filter uses the tenant-first indexes without help from the policy. Two independent layers must both fail before a leak occurs.

**One database role for each tenant.** A role is harder to forge than a setting. This fails R5, and a pooler in transaction mode cannot change roles safely for each transaction.

**One partition for each tenant.** This fails R5.

## Impacts

### Infrastructure

- Three database roles and one secret for each role.
- A migration job that runs as `strata_owner`. The API and the workers never use that role.
- No new service and no added cost.
- One metric: the count of queries that PostgreSQL rejected because the tenant context was absent. A value above zero shows a programming error.
- Rollout: this design is part of the first schema. There is no earlier state to roll back to.

### Security and tenant isolation

Row-level security protects against programming errors. It does not protect against a compromised process:

- The API and the workers can set any tenant context. An attacker who controls one of these processes reaches all tenants.
- An attacker who injects SQL can call `set_config` and reaches all tenants. Thus all queries must use bound parameters. The code builds no SQL text from request input.
- A parser reads hostile files. Issue #7 keeps parsers in a process that has no database credentials.

Row-level security does not apply to these cases. The design closes each one:

| Case | How the design closes it |
|---|---|
| The table owner | Each table forces row-level security. Only the migration job uses the owner role. |
| A superuser, or a role that can bypass row-level security | No runtime role has these rights. |
| `TRUNCATE` | No runtime role has the right to truncate. |
| A view | Each view on a tenant-owned table sets `security_invoker`. |
| A function that runs with owner rights | The design has none. |
| A unique constraint that spans tenants | Each unique constraint on a tenant-owned table starts with `tenant_id`. |

### Data and migration

Each table must be classified as tenant-owned or global when a migration creates it. Test case TC-3-06 fails the build if a table is in neither class.

### Developer

- A developer needs a local PostgreSQL in a container.
- Tests that touch tenant-owned tables open a tenant transaction through one test helper.

## Test cases

L0: none. This design is database behaviour, and L0 tests use no database.

L3: none. The tenant context adds one statement to each transaction. Later designs measure that cost in their own L3 test cases.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-3-01 | L1 | Given tenants A and B each own one row. When a transaction in the tenant context of A selects all rows with no tenant filter. Then the result contains only the row of A. | (not decided) | pending |
| TC-3-02 | L1 | Given no tenant context. When a runtime role selects from a tenant-owned table. Then the result is empty. When the role inserts a row. Then PostgreSQL rejects the insert. | (not decided) | pending |
| TC-3-03 | L1 | Given the tenant context of A. When a transaction inserts a row with the `tenant_id` of B. Then PostgreSQL rejects the insert. | (not decided) | pending |
| TC-3-04 | L1 | Given the tenant context of A. When a transaction inserts a child row of A that points at a parent row of B. Then PostgreSQL rejects the insert. | (not decided) | pending |
| TC-3-05 | L1 | Given a pooler in transaction mode with one server connection. When a transaction of A ends and a transaction of B starts on that connection. Then the transaction of B reads only rows of B. | (not decided) | pending |
| TC-3-06 | L1 | For each table in the schema. Then the table is in the list of global tables, or it has a `tenant_id` column, forced row-level security and a policy for all commands. | (not decided) | pending |
| TC-3-07 | L1 | For each runtime role. Then the role is not a superuser, cannot bypass row-level security, owns no table and cannot truncate a table. | (not decided) | pending |
| TC-3-08 | L1 | Given an account of tenant A. When the API receives its login identity. Then the API opens a tenant transaction for A. When the API receives an identity that is not known. Then the API rejects the request. | (not decided) | pending |
| TC-3-09 | L1 | Given a tenant that is not active. When one of its accounts sends a request. Then the API rejects the request and runs no further query. | (not decided) | pending |
| TC-3-10 | L2 | Given tenants A and B each own a document that contains a word no other document contains. When an account of A searches for the word of B through the public API. Then the response contains no result and no count that reveals the document of B. | (not decided) | pending |

## Unresolved questions

The owner must answer these questions before this design can be accepted:

1. Can two accounts share one login identity? An example is one person who uses the same login for two organisations. This design assumes no, and the primary key of `login_identities` enforces that. If the answer is yes, the login step must ask which tenant to use.
2. What does a tenant that is not active mean? This design assumes that the API rejects all of its requests and that Strata keeps its data.
3. Who creates tenants and accounts in v1? This design assumes that an operator creates them with a command, and that there is no self-service sign-up.

Implementation resolves these points:

- The UUID version.
- The exact SQL text of the policy.
- The test helper that opens a tenant transaction.
