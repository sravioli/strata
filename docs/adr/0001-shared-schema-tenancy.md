---
status: "accepted"
date: 2026-11-15
decision-makers: sravioli
tech-design: docs/tech-design/0003-tenancy-model.md
---

# Shared schema with forced row-level security for tenancy

## Context and Problem Statement

Strata keeps the documents of many organisations in one system, and the number of tenants is not known. An account must never read or change the data of a different tenant, also when a client changes a request. Which tenancy model gives that boundary for 1 tenant and for 50,000 tenants? The issue is [#3](https://github.com/sravioli/strata/issues/3).

## Considered Options

* Shared schema with forced row-level security
* One schema or one database for each tenant
* One shared schema with application filters only
* One database role for each tenant
* One partition for each tenant

## Decision Outcome

Chosen option: "Shared schema with forced row-level security", because it alone costs one row for each tenant and has more than one layer of enforcement.

The decision has these parts:

* A tenant is an organisation. An account belongs to exactly one tenant.
* Each tenant-owned table has `tenant_id` first in its primary key.
* Three layers enforce the boundary: a tenant transaction in the application, one row-level security policy on each table, and composite foreign keys.
* After login, the tenant of a request comes from a signed access token (JWT) only.
* A username is unique in one tenant only. A client sends the tenant name at login.
* A runtime role cannot bypass row-level security and cannot list the names of tenants.
* An operator creates tenants and accounts in v1.

### Consequences

* Good, because a new tenant needs no schema, table, partition, index or role.
* Good, because a query with no tenant filter exposes no rows of other tenants.
* Good, because no process that serves requests needs the rights of the owner role.
* Bad, because the boundary depends on policies and tests, not on physical separation.
* Bad, because row-level security does not stop a compromised API or worker process, or SQL injection.
* Bad, because Strata cannot restore or remove the data of one tenant alone.
* Bad, because a tenant that needs stronger isolation needs a separate deployment.
* Not decided: who issues the access token. Milestone M1 decides it.
