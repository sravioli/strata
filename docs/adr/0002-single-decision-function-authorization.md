---
status: "accepted"
date: 2026-11-18
decision-makers: sravioli
tech-design: docs/tech-design/0004-v1-authorization.md
---

# One pure decision function and an authorized scope for each access

## Context and Problem Statement

In v1 each account of a tenant can see each document of that tenant. Milestone M4 must give each account smaller scopes. Where does Strata decide each access, so that M4 changes one rule and not each route and each search? The issue is [#4](https://github.com/sravioli/strata/issues/4).

## Considered Options

* One pure decision function and an authorized scope in each retrieval function
* Check the tenant only in the database with row-level security
* Add the checks in M4
* A policy engine, for example Cedar or Open Policy Agent
* A check after the query

## Decision Outcome

Chosen option: "One pure decision function and an authorized scope in each retrieval function", because M4 then changes only one rule. Lean can prove that rule.

The decision has these parts:

* `authorize(principal, action, resource)` gives "allow" or "deny". It is in the pure core crate and reads no database and no clock.
* `authorized_scope(principal, action)` gives the set of resources that a principal can reach. Only this function creates a scope.
* Each retrieval function takes a scope and filters by it in its query.
* A tenant transaction opens only with an "allow" decision or a scope. Before the decision, the identity transaction reads only the issuer binding, the account and the statuses. Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)): the identity transaction is new.
* In v1 an active account of an active tenant can do each of the five account actions on the data of its tenant.
* A worker reads the tenant status when it claims a job. It does not start a job of a tenant that is not active. A job that runs continues to its end.
* If the status read uses a cache, a change of status removes the old status from the cache.

### Consequences

* Good, because M4 changes the rule and adds data, and the callers do not change.
* Good, because the decision function and row-level security are two independent checks of the tenant boundary.
* Good, because a scope in the query keeps counts and page sizes correct, also for vector search.
* Bad, because each route and each retrieval function needs a decision or a scope, for a v1 rule that one line can express.
* Bad, because each request reads the status of the account and of the tenant.
* Bad, because the proofs do not show that the API builds the principal correctly, or that a retrieval function obeys its scope. Test cases cover those.
* Not decided: the scope unit of M4, and if a denied job waits in the queue or fails ([#5](https://github.com/sravioli/strata/issues/5)). Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)): ADR 0004 (job queue) decided that the job becomes `Parked`.
