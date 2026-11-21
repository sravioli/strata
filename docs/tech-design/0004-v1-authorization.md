- Issue: [#4](https://github.com/sravioli/strata/issues/4)
- Status: accepted
- Start date: 2026-11-15
- Decided by: sravioli, 2026-11-18

# V1 authorization: tenant-wide now, scoped later

## Summary

One pure function decides each access in Strata. In v1 its rule is: an active account of an active tenant can do each account action on the data of its tenant. Each retrieval function takes an authorized scope as an argument. A retrieval function is a function that searches documents. In v1 the scope is the tenant. Milestone M4 adds smaller scopes for each account, and it changes the rule, not the callers.

## Motivation

The owner made two decisions on 2026-11-14:

- In v1, each account of a tenant can see each document of that tenant.
- Later, an account must see only the documents of its scopes. Milestone M4 owns that work.

If v1 puts access checks in many places, M4 must find and change each one. A place that M4 does not find is a leak. This design puts the decision in one function from the start. Then M4 changes one function and adds data. It does not change the retrieval code.

The tenancy design ([#3](https://github.com/sravioli/strata/issues/3)) keeps the data of a tenant apart from other tenants. This design decides what an account can do inside its tenant. The two designs give two independent checks of the tenant boundary.

## Guide-level explanation

### Terms

- **Principal**: the actor of an action. It is an account or a job.
- **Action**: one operation that a principal requests.
- **Resource**: the data that an action reads or changes. Each resource belongs to one tenant.
- **Decision function**: the function that gives "allow" or "deny" for a principal, an action and a resource.
- **Authorized scope**: the set of resources that a principal can reach with an action. A retrieval function reads only inside this set.

### Examples

Anna and Bruno have accounts in the tenant Acme. Carla has an account in the tenant Birch. All accounts and tenants are active.

- Anna uploads a document. Bruno searches and finds it. Each account of Acme can see each document of Acme.
- Bruno removes the document of Anna. In v1, each active account can do each account action.
- Carla searches. Her authorized scope is the tenant Birch. The search reads only documents of Birch.
- Carla requests the document of Anna by its identifier. The decision function gives "deny". The API answers as for an identifier that does not exist.
- An operator sets the account of Bruno to not active. The access token of Bruno is not expired. The next request of Bruno gets "deny".
- A job processes the document of Anna. The job is a principal of the tenant Acme. It can process documents of Acme only.
- A job of Acme runs. An operator sets Acme to not active. The job that runs continues to its end. The next job of Acme that a worker claims gets "deny".

### How to think about it

A developer who adds an API route does three steps:

1. The route gets the principal from the verified access token.
2. The route calls the decision function, or it gets an authorized scope.
3. The route gives the result to the function that reads or changes data.

The data functions accept no call that skips step 2. The Rust type system enforces that rule.

## Reference-level explanation

### Principals

| Principal | Source | Content |
|---|---|---|
| Account | The verified access token, and the status that the API reads on each request | `tenant_id`, `account_id`, account status, tenant status |
| Job | A job that a worker claimed, and the tenant status that the worker reads when it claims the job | `tenant_id`, tenant status |

A client cannot supply a principal. The tenancy design gives the rules for the access token and for the tenant of a job.

The operator command is not a principal. It does not use the API, and the tenancy design gives its rights.

### Actions

| Action | Principal kind | Meaning |
|---|---|---|
| `Search` | Account | Search the documents of the tenant. |
| `ReadDocument` | Account | Read one document or its state. |
| `UploadDocument` | Account | Add a document or a new revision. |
| `DeleteDocument` | Account | Remove a document. |
| `ProcessDocument` | Job | Extract and index a revision. |

A later design that adds an operation adds an action to this list. An operation with no action cannot reach data.

### The decision function

The function is `authorize(principal, action, resource)`. It gives "allow" or "deny". It is in the pure core crate. It reads no database and no clock.

The v1 rule has two parts. The function gives "allow" only when the two parts are true.

Part 1, the rule permits the action for the principal:

| Principal | Condition |
|---|---|
| Account | The account is active, the tenant is active, and the action is an account action. |
| Job | The tenant is active, and the action is `ProcessDocument`. |

Part 2, the resource belongs to the tenant of the principal.

All other cases give "deny". A "deny" carries one of three reasons for the log:

- The principal is not active.
- The rule does not permit the action.
- The resource is of a different tenant.

### The authorized scope

A search cannot call the decision function for each document. Thus a second function, `authorized_scope(principal, action)`, gives the set of resources that the principal can reach.

In v1 the scope has two forms:

- `Nothing`: the principal can reach no resource.
- `Tenant(tenant_id)`: the principal can reach each resource of that tenant.

The scope is "Tenant" when part 1 of the rule is true. If part 1 is false, the scope is "Nothing".

Three rules apply to the scope:

1. Only `authorized_scope` creates a scope. Other code cannot build one.
2. The function that opens a tenant transaction accepts only an "allow" decision or a scope. Thus no query on a tenant-owned table runs before a decision. The identity transaction is the only exception (see "Order of the checks in a request").
3. Each retrieval function takes a scope as an argument: lexical search, vector search and the fusion of the two. Each one filters by the scope in its query. No retrieval function filters its results after the query.

Rule 3 is important for vector search. A filter after the query can return fewer results than the client requested, and it can reveal the count of hidden documents.

### What the client receives

| Case | Response |
|---|---|
| The account or its tenant is not active | The API rejects the request. It runs no query on documents. |
| The resource is of a different tenant | The same response as for an identifier that does not exist. |
| The action is not permitted for the principal | The API rejects the request. |

The reason for a "deny" goes to the log only.

A worker that gets "deny" for a job does not start the job. The job queue design ([#5](https://github.com/sravioli/strata/issues/5)) decides if the job waits in the queue or fails. Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)): ADR 0004 (job queue) decided that the job becomes `Parked`.

### Order of the checks in a request

1. The API verifies the access token.
2. The API opens the identity transaction, and reads the status of the account and of the tenant in it.
3. The API builds the principal.
4. The API calls `authorize` or `authorized_scope`.
5. The API gives the result to the function that opens a tenant transaction. That function changes the identity transaction into a tenant transaction. Then the API runs the queries.

The status rows are tenant-owned, so row-level security permits their read only in a tenant context. The **identity transaction** gives that context before a decision exists:

- It is a transaction on the primary. Its tenant context is the tenant of the issuer binding of the verified token ([#21](https://github.com/sravioli/strata/issues/21)). Thus R2 of the tenancy design holds.
- It can do only three things:
  - read the issuer binding,
  - read or create the account,
  - read the status of the account and of the tenant.
- No other query accepts it as its database handle. The Rust type system enforces that rule.

A read that goes to a replica (ADR 0003, capacity, S2) runs in a second tenant transaction on the replica. The API opens it with the same decision or scope.

A worker does the same steps at the claim of a job. The claim transaction reads the job, sets the tenant context of the job, and reads the tenant status. That is the identity transaction of the worker. Then `job_step` and `authorize` decide, and the worker opens the tenant transaction of the job with the result.

Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)): the identity transaction is new. Before it, the order of the checks in this design, in the tenancy design and in the identity provider design did not agree.

Step 2 can use a cache. If it uses a cache, the operator command that changes a status must remove the old status from the cache. Then the next request after the change gets the new status, and test case TC-4-04 passes with no delay.

### How M4 changes this design

M4 adds per-account scopes. The expected changes are:

- A new form of scope that names a set of scope units of one tenant.
- New data: which account has which scope unit.
- A new part of the rule in `authorize` and `authorized_scope`.

The signatures of the retrieval functions do not change. The form of the scope unit is a decision of M4.

## Proof obligations

Each theorem is about the Rust functions `authorize` and `authorized_scope` in the core crate. These functions use only enums, structs, `match` and integer comparison. Spike [#8](https://github.com/sravioli/strata/issues/8) showed that Aeneas translates such code.

| Theorem | Requirement it covers | Plain-English statement | Lean statement | Rust function it is about |
|---|---|---|---|---|
| `allow_same_tenant` | Tenant boundary | If the decision is "allow", the resource belongs to the tenant of the principal. | `authorize p a r = true → r.tenant = p.tenant` | `authorize` |
| `allow_account_active` | The v1 rule | If the decision is "allow" for an account, the account is active and its tenant is active. | `authorize (.account t c aa ta) a r = true → aa = true ∧ ta = true` | `authorize` |
| `active_account_allowed` | Tenant-wide visibility in v1 | An active account of an active tenant gets "allow" for each account action on each resource of its tenant. | `a.forAccount = true → r.tenant = t → authorize (.account t c true true) a r = true` | `authorize` |
| `account_never_processes` | Separation of accounts and jobs | An account never gets "allow" for `ProcessDocument`. | `authorize (.account t c aa ta) .processDocument r = false` | `authorize` |
| `job_only_processes` | Separation of accounts and jobs | If the decision is "allow" for a job, the action is `ProcessDocument` and the resource belongs to the tenant of the job. | `authorize (.job t ta) a r = true → a = .processDocument ∧ r.tenant = t` | `authorize` |
| `allow_job_active` | A tenant that is not active starts no new job | If the decision is "allow" for a job, the tenant of the job is active. | `authorize (.job t ta) a r = true → ta = true` | `authorize` |
| `scope_agrees` | Scope and decision are one rule | A resource is in the authorized scope exactly when the decision for that resource is "allow". | `(authorizedScope p a).contains r = authorize p a r` | `authorized_scope`, `authorize` |
| `scope_one_tenant` | Tenant boundary | Each resource in an authorized scope belongs to the tenant of the principal. | `(authorizedScope p a).contains r = true → r.tenant = p.tenant` | `authorized_scope` |
| `authorize_total` | No failure | The two functions give a result for each input and never stop with an error. | Part of the proof that the Rust code agrees with the model | `authorize`, `authorized_scope` |

A draft Lean model of the rule proves the first seven statements. The model has no tenant status for a job yet. It must get that status before it proves `allow_job_active`. The model is not in the repository. Implementation writes the model again in the repository and proves the statements on the Rust code.

Assumptions:

- The principal is correct. The proofs do not show that the API builds the principal from a verified token. Test cases cover that.
- The status in the principal is the status at the time of the read. A change of status after the read applies to the next request. For a job, it applies to the next job that a worker claims.
- The proofs do not show that a retrieval function obeys its scope. Test cases cover that.
- Issue [#9](https://github.com/sravioli/strata/issues/9) decides the proof route. Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)): ADR 0007 (verification boundary) chose route 1 for these functions. If the route is a model with differential tests, the statements stay the same and a differential test compares the Rust code with the model.
- `active_account_allowed` is true for v1 only. M4 replaces it.

## Impacts

### Infrastructure

- No new service, storage or secret.
- One metric: the count of "deny" decisions for each reason. An increase of "different tenant" shows a programming error or an attack.
- A log entry for each "deny", with the principal, the action and the reason. The entry contains no document content.
- Rollout: this design is part of the first version of the API. There is no earlier state.

### Security and tenant isolation

- The decision function compares the tenant of the resource with the tenant of the principal. Row-level security makes the same comparison in the database. A leak needs a failure of the two.
- A compromised API process can build any principal. This design does not protect against that case, and the tenancy design says the same.
- New input that is not trusted: none. The principal comes from the verified token and from the database. The action comes from the route, not from the request.
- A response never shows the reason for a "deny", and it never shows that a resource of a different tenant exists.

### Data and migration

N/A. This design adds no table and no column. It reads the status columns that the tenancy design defines.

## Test cases

L0: unit tests cover each row of the decision rule, for each principal kind and each action.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-4-01 | L1 | Given accounts Anna and Bruno of one tenant, and a document that Anna uploaded. When Bruno searches for the document and reads it. Then Bruno finds it and reads it. | sravioli, 2026-11-18 | pending |
| TC-4-02 | L1 | Given accounts Anna and Bruno of one tenant, and a document that Anna uploaded. When Bruno removes the document. Then the API removes it. | sravioli, 2026-11-18 | pending |
| TC-4-03 | L1 | Given an account that is not active in an active tenant. When the account sends a request for each account action. Then the API rejects each request and runs no query on documents. | sravioli, 2026-11-18 | pending |
| TC-4-04 | L1 | Given an active account with an access token that is not expired. When an operator sets the account to not active, and the account sends a request. Then the API rejects the request. | sravioli, 2026-11-18 | pending |
| TC-4-05 | L1 | Given documents in tenants A and B. When lexical search, vector search and the fusion of the two each run with the scope of tenant A. Then each result is a document of A, and each count is a count of documents of A. | sravioli, 2026-11-18 | pending |
| TC-4-06 | L1 | Given the scope "Nothing". When each retrieval function runs with it. Then each result is empty and the function runs no query. | sravioli, 2026-11-18 | pending |
| TC-4-07 | L1 | Given a job of tenant A. When the worker processes a document of A. Then the decision is "allow". When the content of the job names a document of B. Then the decision is "deny". | sravioli, 2026-11-18 | pending |
| TC-4-08 | L1 | For each route of the API. Then the route gets its tenant transaction only from a decision or a scope. Before the decision, the route uses only the identity transaction, for the binding, the account and the statuses. The build fails if a route opens a tenant transaction in a different way, or runs a different query in the identity transaction. Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)). | sravioli, 2026-11-18 | pending |
| TC-4-09 | L2 | Given two active accounts of one tenant and one account of a different tenant. When each account uploads a document and then searches through the public API. Then the two accounts of one tenant see the same two documents. The third account sees only its own document. | sravioli, 2026-11-18 | pending |
| TC-4-10 | L1 | Given a job of tenant A that runs. When an operator sets A to not active. Then the job continues to its end. When a worker claims the next job of A. Then the decision is "deny" and the worker does not start the job. | sravioli, 2026-11-18 | pending |

Test cases TC-4-05 and TC-4-06 stay `pending` for vector search and fusion until milestone M2 adds them.

## Drawbacks

- In v1 the decision function is small. The design adds types and one call to each route for a rule that one line can express.
- A scope argument on each retrieval function is more text in each signature.
- The status read on each request costs one more query, or two. A worker also reads the tenant status for each job.

## Rationale and alternatives

**Check the tenant only in the database.** Row-level security already keeps tenants apart. But it cannot express the rules of M4 without a policy for each scope, and it gives no single place for the decision. This design keeps row-level security as the second check.

**Add the checks in M4.** This is the least work in v1. M4 must then change each route and each retrieval function, and a place that it does not find is a leak. The owner said that scopes are necessary later, so the cost comes for certain.

**A policy engine or a policy language.** Examples are Cedar and Open Policy Agent. They give flexible rules. But v1 has one rule, the rule changes one time in M4, and an engine is a new dependency outside the proved core. A Rust function is smaller and Lean can prove it.

**A check after the query.** The code reads rows and then removes the rows that the account cannot see. This breaks counts and page sizes, and it is slow for vector search. Thus the scope goes into the query.

**Roles in v1.** Examples are a read-only account and a tenant administrator. The owner did not request roles. They can go in the same decision function later.

If Strata does not do this, v1 works the same for a client. The cost comes in M4.

## Prior art

- Cedar (https://www.cedarpolicy.com) uses the same three inputs: principal, action and resource. Its authors verified its evaluator with Lean.
- Google Zanzibar (https://research.google/pubs/zanzibar-googles-consistent-global-authorization-system/) stores relations between accounts and objects. It is a model for the scope data of M4, not for v1.
- PostgreSQL row-level security is the second check that the tenancy design already uses.

## Unresolved questions

The owner answered the three questions of this design on 2026-11-18:

1. Each active account can do each account action in v1, with `UploadDocument` and `DeleteDocument` included. A later design can make this rule smaller.
2. A job that runs when its tenant becomes not active continues to its end. A worker does not start a new job of a tenant that is not active.
3. The list of five actions is correct for v1.

Implementation resolves these points:

- The Rust types for the principal, the decision and the scope.
- If the status read uses a cache. If it does, a change of status removes the old status from the cache (see "Order of the checks in a request").

These questions belong to later designs:

- The scope unit: a collection of documents or one document. Milestone M4 decides ([#22](https://github.com/sravioli/strata/issues/22)).
- Roles inside a tenant ([#66](https://github.com/sravioli/strata/issues/66)).
- Authorization for an operation that a later design adds.
- If a job that gets "deny" waits in the queue or fails. Issue [#5](https://github.com/sravioli/strata/issues/5) decides. Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)): it becomes `Parked` (ADR 0004, job queue).

## Future possibilities

- A scope that names collections, for M4 ([#22](https://github.com/sravioli/strata/issues/22)).
- A read-only kind of account ([#66](https://github.com/sravioli/strata/issues/66)).
- A tenant administrator who creates accounts through the API ([#66](https://github.com/sravioli/strata/issues/66)).
- An audit record of each "allow" decision for sensitive actions ([#67](https://github.com/sravioli/strata/issues/67)).
