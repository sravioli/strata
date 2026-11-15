- Issue: [#3](https://github.com/sravioli/strata/issues/3)
- Status: draft
- Start date: 2026-11-14
- Decided by: (not decided)

# Tenancy model

## Problem description

Strata is a backend service that keeps the documents of many organisations in one system. It has no user interface. An account must read and change only the data of its own tenant. The number of tenants is not known. The goal is approximately 50,000 accounts. This number is an estimate, not a measurement.

Goals:

- One model that works for any number of tenants and any tenant size.
- A tenant boundary that more than one layer enforces.
- A request that a client changed must not reach the data of a different tenant.
- A model small enough to describe in Lean.

Non-goals:

- Authorization inside a tenant. Issue [#4](https://github.com/sravioli/strata/issues/4) owns that decision.
- The identity provider and the storage of passwords. Milestone M1 owns those decisions.
- The signing key, the lifetime and the renewal of an access token. Milestone M1 owns those decisions.
- A user interface. A later frontend for demonstrations is a client of the API.
- Dedicated infrastructure for one tenant.
- The layout of the vector index. Milestone M2 owns that decision.
- The removal of a tenant and its data.

## Background and references

The owner made two decisions on 2026-11-14:

- A **tenant** is an organisation. It is a group of several people.
- An **account** belongs to exactly one tenant. A person who works with two organisations has one account in each.

The owner made six decisions on 2026-11-15:

- At login, the client sends the name of the tenant, a username and a password. Thus a username is unique in one tenant only.
- Strata is a backend only. A client authenticates each request with a JSON Web Token (JWT).
- The total of approximately 50,000 accounts is an estimate and a goal.
- When a tenant is not active, the API rejects all of its requests. Strata keeps its data.
- In v1, an operator creates tenants and accounts with a command. There is no self-service sign-up.
- Strata must not reveal the names of tenants.

References:

- PostgreSQL documentation, "Row Security Policies": https://www.postgresql.org/docs/current/ddl-rowsecurity.html
- PostgreSQL documentation, "Configuration Settings Functions" (`set_config`, `current_setting`): https://www.postgresql.org/docs/current/functions-admin.html
- PgBouncer documentation, "Features" (pooling modes): https://www.pgbouncer.org/features.html
- RFC 7519, "JSON Web Token (JWT)": https://www.rfc-editor.org/rfc/rfc7519
- RFC 8725, "JSON Web Token Best Current Practices": https://www.rfc-editor.org/rfc/rfc8725

Terms used in this design:

- **Tenant-owned table**: a table whose rows each belong to one tenant.
- **Global table**: a table whose rows belong to no tenant.
- **Tenant context**: the tenant that a database transaction acts for.
- **Access token**: a signed JWT that a client sends with each request. It names one tenant and one account.
- **Runtime role**: a database role that the API or a worker uses.
- **Owner role**: the database role that owns the tables and applies migrations.
- **Operator role**: the database role that an operator uses for tasks that span tenants.

## Requirements

| ID | Requirement |
|---|---|
| R1 | A transaction reads and changes tenant-owned rows of one tenant only. |
| R2 | After login, the tenant of a request comes from the verified access token only. It never comes from other request input. |
| R3 | A transaction with no tenant context gets no tenant-owned rows. |
| R4 | A query that omits the tenant filter because of a programming error exposes no rows of other tenants. |
| R5 | A new tenant costs one row. The design creates no schema, table, partition, index or role for a tenant. |
| R6 | The model works through a connection pooler in transaction mode. |
| R7 | A worker processes jobs of all tenants. Each job runs in the tenant context of exactly one tenant. |
| R8 | The tenant name that a client gives at login is a claim. It gives no access until the API verifies the password. |
| R9 | A runtime role cannot list the names of tenants. No response shows that a tenant name exists. |

Quantities:

- Accounts: approximately 50,000. This is an estimate and a goal.
- Tenants: not known. The design must hold from 1 tenant to 50,000 tenants.
- Tenant size: not known. The design must hold when one tenant owns most of the data.

## Proposed design

### Data model

Two tables carry the model:

- `tenants(tenant_id, name, status, created_at)`. The `name` is unique. It is the name that a client sends at login.
- `accounts(tenant_id, account_id, username, status, created_at)` is tenant-owned. The pair `(tenant_id, username)` is unique.

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

The code never reads a tenant identifier from a header, a query parameter, a path or a request body.

**Layer 2: row-level security.** Each tenant-owned table has row-level security enabled and forced. Each table has exactly one policy, for all commands:

```sql
using      (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
with check (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
```

With no tenant context the expression is null. PostgreSQL then returns no rows and rejects each write. The `nullif` call is necessary. After a transaction sets the value, PostgreSQL gives an empty string, not null, to later transactions on that connection.

A table must not have a second policy. PostgreSQL gives access when one of several policies permits it. Thus a second policy can expose the rows of all tenants.

**Layer 3: constraints.** The composite foreign keys reject a child row that points at a parent row of a different tenant.

### The `tenants` table

The login step must find a tenant by its name before a tenant context exists. The `tenants` table has its own policy for that step. It has row-level security enabled and forced, and exactly one policy, for `select` only:

```sql
using (
  tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid
  or name = nullif(current_setting('app.login_tenant', true), '')
)
```

A runtime role thus reads a row of `tenants` in two cases only:

- The row is the tenant of the tenant context.
- The transaction gives the exact name of the tenant.

A runtime role has only the right to select from `tenants`. Only the operator role adds a tenant or changes its status.

### Database roles

| Role | Used by | Rights |
|---|---|---|
| `strata_owner` | The migration job only | Owns all tables. Applies migrations. |
| `strata_api` | The API | Reads and writes rows. Owns no table. Is not a superuser. Cannot bypass row-level security. Cannot truncate. Cannot create roles or objects. Is a member of no other role. |
| `strata_worker` | Workers | The same limits as `strata_api`. |
| `strata_operator` | The operator command and logical backups | Can bypass row-level security. |

The policies also apply to `strata_owner`, because each table forces row-level security.

Two roles can bypass row-level security: the PostgreSQL superuser and `strata_operator`. The environment of the API and of the workers holds the credentials of neither role.

The operator command does these tasks:

- It creates a tenant.
- It creates an account.
- It changes the status of a tenant or of an account.
- It lists tenants.

### Global tables

A global table has no row-level security. A global table must not hold tenant content. It can hold identifiers and state.

This design has no global table. The job queue of issue [#5](https://github.com/sravioli/strata/issues/5) is an example of a global table. A worker claims a job, reads its `tenant_id`, then opens a tenant transaction for that tenant.

No database function runs with the rights of the owner role.

### Login flow

1. The client sends a tenant name, a username and a password.
2. The API starts a transaction and runs `select set_config('app.login_tenant', $1, true)` with the tenant name.
3. The API reads the row of `tenants` with that name. It gets the `tenant_id` and the status.
4. The API sets the tenant context to that `tenant_id` in the same transaction.
5. The API reads the account with that `(tenant_id, username)`.
6. The API verifies the password. Milestone M1 decides how.
7. The API rejects the login if the tenant or the account is not active.
8. The API issues an access token that carries the `tenant_id` and the `account_id`.

The API gives the same response in each of these cases:

- The tenant name is not known.
- The username is not known in that tenant.
- The password is incorrect.
- The tenant or the account is not active.

The API verifies a password hash in each case. Thus the response time does not show which case occurred.

### Access token

The access token is the only source of the tenant after login. These rules apply:

- Strata signs the token. The token carries one `tenant_id`, one `account_id` and an expiry time.
- A client cannot change the token, because a change breaks the signature.
- The API verifies the signature and the expiry time before it runs a query on a tenant-owned table.
- The API accepts one signing algorithm only. It rejects a token that names a different algorithm or no algorithm.
- The API rejects a token that fails verification or whose expiry time passed.

A token stays valid until its expiry time. Strata cannot cancel it earlier. Thus the API reads the status of the account and of the tenant on each request.

### Request flow

1. The API verifies the access token and gets the tenant and the account from it.
2. The API opens a tenant transaction for that tenant.
3. The API reads the status of the account and of the tenant. It rejects the request if one of them is not active.
4. The API runs its queries in that transaction.

The API reads no tenant identifier from a different part of the request.

When a request names a row of a different tenant, the query finds no row. The API gives the same response as for an identifier that does not exist.

### Connection pooler

The tenant context applies to one transaction. A pooler in transaction mode gives a connection to one transaction at a time. Thus a later transaction on the same connection does not get the earlier tenant context.

The code sets `app.tenant_id` and `app.login_tenant` only with the third argument of `set_config` set to `true`. A value that applies to the database session stays on the connection for the next client.

### Isolation tiers

This design has one tier: all tenants share one deployment. A tenant that needs stronger isolation gets a separate deployment of the complete system. This design adds no code for that case.

## Alternatives considered

**One schema or one database for each tenant.** The boundary is strong. This fails R5, and the tenant count is not known. The costs increase with each tenant:

- Each migration runs one time for each tenant. A migration that stops after some tenants leaves tenants on different versions.
- PostgreSQL keeps a catalog entry for each table and each index. With 50,000 tenants, the catalog holds millions of entries.
- A database for each tenant needs its own connections.
- A new tenant needs statements that create tables. Thus a process that runs while Strata serves requests needs the rights of the owner role.

**Shared schema with application filters only.** This is the simplest model. One query with no tenant filter exposes all tenants. This fails R4.

**Row-level security with no filter in the queries.** This meets R1 to R4. The design keeps the explicit filter for two reasons. The filter uses the tenant-first indexes without help from the policy. Two independent layers must both fail before a leak occurs.

**One database role for each tenant.** A role is harder to forge than a setting. This fails R5, and a pooler in transaction mode cannot change roles safely for each transaction.

**One partition for each tenant.** This fails R5.

**A global table of login identities.** An earlier draft of this design had a global table from a login identity to an account. That table needs a login identity that is unique across all tenants. The owner decided that the user gives the tenant name at login, so this design removes the table.

**A global `tenants` table.** This is simpler than a policy on `tenants`. But a programming error can then list the names of all tenants. This fails R9.

## Proof obligations

N/A. This design is database behaviour: SQL, row-level security policies and database roles. A Lean proof about a model does not show that PostgreSQL enforces a policy. The L1 and L2 test cases of this design cover that behaviour.

## Impacts

### Infrastructure

- Four database roles and one secret for each role.
- A migration job that runs as `strata_owner`. The API and the workers never use that role.
- An operator command that runs as `strata_operator`. The API and the workers never use that role.
- No new service and no added cost.
- One metric: the count of queries that PostgreSQL rejected because the tenant context was absent. A value above zero shows a programming error.
- Rollout: this design is part of the first schema. There is no earlier state to roll back to.

### Security and tenant isolation

**A changed request.** A client can change each part of a request. The design stops each case:

| The client changes | How the design stops it |
|---|---|
| A tenant identifier in a header, a query parameter, a path or a body | The API reads the tenant from the access token only. |
| The identifier of a row, to the identifier of a row of a different tenant | The filter and the policy find no row. |
| The access token | The API verifies the signature and rejects a token that a client changed. |
| The tenant name at login | The password check fails, because the account is not in that tenant. |
| The `tenant_id` of a row in a write | The policy and the composite foreign keys reject the write. |

**A compromised process.** Row-level security protects against programming errors. It does not protect against a compromised process:

- The API and the workers can set any tenant context. An attacker who controls one of these processes reaches all tenants.
- An attacker who injects SQL can call `set_config` and reaches all tenants. Thus all queries must use bound parameters. The code builds no SQL text from request input.
- A parser reads hostile files. Issue [#7](https://github.com/sravioli/strata/issues/7) keeps parsers in a process that has no database credentials.

**Cases where row-level security does not apply.** The design closes each one:

| Case | How the design closes it |
|---|---|
| The table owner | Each table forces row-level security. Only the migration job uses the owner role. |
| A superuser, or a role that can bypass row-level security | No runtime role has these rights. No runtime role is a member of a role that has them. |
| `TRUNCATE` | No runtime role has the right to truncate. |
| A view | Each view on a tenant-owned table sets `security_invoker`. |
| A function that runs with owner rights | The design has none. |
| A unique constraint that spans tenants | Each unique constraint on a tenant-owned table starts with `tenant_id`. |
| A second policy on a table | Test case TC-3-06 fails the build if a table has more than one policy. |

**Tenant names.** The design keeps the names of tenants secret in four ways:

- A runtime role reads a row of `tenants` only with the exact name or in the tenant context of that tenant.
- The login step gives one response for all causes of failure.
- The API has no endpoint that lists or searches tenants.
- A response or an error message never contains the name of a different tenant.

### Data and migration

Each table must be classified as tenant-owned or global when a migration creates it. Test case TC-3-06 fails the build if a table is in neither class.

### Developer

- A developer needs a local PostgreSQL in a container.
- Tests that touch tenant-owned tables open a tenant transaction through one test helper.
- Each new API route needs a test that uses identifiers of a different tenant. Test case TC-3-18 fails the build if a route has none.

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
| TC-3-06 | L1 | For each table in the schema. Then the table is in the list of global tables, or it has forced row-level security and exactly one policy. The text of that policy is equal to the text in this design for a tenant-owned table or for `tenants`. A tenant-owned table also has a `tenant_id` column. | (not decided) | pending |
| TC-3-07 | L1 | For each runtime role. Then the role is not a superuser and cannot bypass row-level security. The role cannot create roles or objects in the schema. The role owns no table, cannot truncate a table and is a member of no other role. | (not decided) | pending |
| TC-3-08 | L1 | Given an account of tenant A. When the client logs in with the name of A, the username and the password. Then the API issues an access token that carries A and the account. | (not decided) | pending |
| TC-3-09 | L1 | Given a tenant that is not active. When one of its accounts sends a request. Then the API rejects the request and runs no further query. | (not decided) | pending |
| TC-3-10 | L2 | Given tenants A and B each own a document that contains a word no other document contains. When an account of A searches for the word of B through the public API. Then the response contains no result and no count that reveals the document of B. | (not decided) | pending |
| TC-3-11 | L1 | Given an access token of an account of A. When a request carries the `tenant_id` of B in a header, in a query parameter, in the path or in the body. Then the API acts for A in each case. | (not decided) | pending |
| TC-3-12 | L1 | Given an access token of an account of A and a row of B. When the account reads, changes or removes the row by its identifier. Then each response is equal to the response for an identifier that does not exist. The row of B does not change. | (not decided) | pending |
| TC-3-13 | L1 | Given an access token of an account of A. When a client changes the tenant in the token, breaks its signature, names a different algorithm or uses it after its expiry time. Then the API rejects the request and runs no query on a tenant-owned table. | (not decided) | pending |
| TC-3-14 | L1 | Given an account of A. When a client logs in with the name of B and the username and password of that account. Then the API rejects the login. The responses for a tenant name that is not known, a username that is not known and an incorrect password are equal. | (not decided) | pending |
| TC-3-15 | L1 | Given the tenant context of A and a row of A. When a transaction updates the `tenant_id` of the row to B. Then PostgreSQL rejects the update. | (not decided) | pending |
| TC-3-16 | L1 | Given an access token of an account of A. When a string parameter of a request contains SQL text that sets the tenant context to B. Then the API uses the text as data. The tenant context stays A. | (not decided) | pending |
| TC-3-17 | L1 | Given a job of tenant A whose content names a document of B. When a worker runs the job. Then the worker finds no document and reads no row of B. | (not decided) | pending |
| TC-3-18 | L1 | For each route of the API. Then a test calls the route with identifiers of a different tenant. The build fails if a route has no such test. | (not decided) | pending |
| TC-3-19 | L2 | Given tenants A and B each own data. When an account of A calls each endpoint of the public API with identifiers of B. Then no response contains data of B or a count that reveals data of B. | (not decided) | pending |
| TC-3-20 | L1 | Given tenants A and B. When a runtime role selects all rows of `tenants` in the tenant context of A. Then the result contains only the row of A. When the role selects with no tenant context and no login name. Then the result is empty. When the role selects with the login name of B. Then the result contains only the row of B. | (not decided) | pending |

## Unresolved questions

The owner must answer this question before this design can be accepted:

1. Does Strata issue the access token, or does an external identity provider issue it? This design assumes that Strata issues it at the login step. If an external provider issues it, the login flow moves to that provider. The token must then carry a `tenant_id` that Strata can trust.

The owner answered the three questions of the earlier draft on 2026-11-15. The answers are in "Background and references".

Implementation resolves these points:

- The UUID version. Version 7 is the recommendation, because its values increase with time and keep index pages together.
- The exact SQL text of the policies.
- The test helper that opens a tenant transaction.
- The rules for a tenant name and a username: permitted characters and letter case.
- A check that the prepared statements of the database driver work through the pooler in transaction mode.
