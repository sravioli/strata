- Issue: [#2](https://github.com/sravioli/strata/issues/2)
- Status: draft
- Start date: 2026-11-18
- Decided by: (not decided)

# Capacity ceiling and scaling stance

## Problem description

Strata must serve approximately 50,000 accounts, and the peak load is not known. This design sets a design ceiling: the worst-case load that the system must be able to reach. It also sets the rules that the code must obey so that more copies of each process give more capacity. The L3 test cases use the ceiling as their target numbers.

Goals:

- One ceiling, in numbers that an L3 test case can cite.
- A short list of rules for the code that follow from the ceiling.

Non-goals:

- Instance sizes, replica counts, hosting and cost. Milestone M3 decides them.
- A forecast of real load. The ceiling is a test target, not a forecast.

## Background and references

The owner stated the position on 2026-11-14 and on 2026-11-15 (map [#1](https://github.com/sravioli/strata/issues/1)):

- Approximately 50,000 accounts in total. This number is an estimate and a goal, not a measurement.
- The number of tenants is not known.
- The peak concurrency is not known. Thus the design must be able to reach the worst case.
- Sizing is a deployment concern. It must change the code only where that is necessary.

The tenancy design ([#3](https://github.com/sravioli/strata/issues/3), ADR 0001) already requires a model that holds from 1 tenant to 50,000 tenants, and a connection pooler in transaction mode. The v1 authorization design ([#4](https://github.com/sravioli/strata/issues/4), ADR 0002) reads the status of the account and of the tenant on each request. Test case TC-4-04 requires that the next request after a change of status gets the new status.

The handoff package has a draft ADR on pooling and read replicas (`docs/adr/draft/ADR-0008-database-pooling-and-read-scaling.md`). It is a draft, and issue [#10](https://github.com/sravioli/strata/issues/10) decides its future. This design uses two of its points: a bound on each pool, and replicas only for reads that can accept lag.

References:

- PgBouncer documentation, "Features": https://www.pgbouncer.org/features.html
- PostgreSQL documentation, "Hot Standby": https://www.postgresql.org/docs/current/hot-standby.html

## Requirements

### The design ceiling

| ID | Quantity | Ceiling | Source |
|---|---|---|---|
| C1 | Accounts that send requests in the same period | 50,000, which is all accounts | Owner, 2026-11-15 |
| C2 | Searches | 10,000 each second. This is one search for each account each 5 seconds. | Issue [#2](https://github.com/sravioli/strata/issues/2) |
| C3 | Tenants | Each target holds for 1 tenant and for 50,000 tenants, with the same total data | Tenancy design |
| C4 | Uploads | Proposal: 50,000 each hour, which is approximately 14 each second. This is one upload for each account each hour. | Owner decides |
| C5 | Documents in the test corpus | Proposal: 5,000,000 documents, which is 100 for each account. One tenant can hold half of all documents. | Owner decides |
| C6 | Search latency at C2 | Proposal: 95 % of searches in 500 ms or less, and 99 % in 1 s or less | Owner decides |

The ceiling is for one deployment of Strata. A search here is one request to the search route. It can contain a lexical search, a vector search and the fusion of the two.

### The rules for the code

| ID | Rule |
|---|---|
| S1 | Each process is stateless. Each copy of a process can serve each request. A request does not need data that an earlier request left in the memory of one copy. |
| S2 | The search route and the document read routes can read from a read replica. The reads in the list "Reads on the primary" always go to the primary. |
| S3 | Each pool, queue, buffer and cache has a bound. |
| S4 | A process that reaches a bound rejects the work in a bounded time, with a defined response. It does not wait without a time limit. |
| S5 | No code path depends on the number of tenants or on the size of one tenant. |
| S6 | Each bound in S3 has a metric: its size, its use, and the count of rejections. |

## Proposed design

### Stateless processes

The API and the worker keep no session in memory. The access token carries the tenant and the account, and each request reads the status from the database (ADR 0002). Thus a load balancer can send each request to each copy, and a new copy starts with no transfer of state.

A copy can keep a cache in its memory only when two conditions are true:

- The loss of the cache changes no result.
- The data in the cache cannot become incorrect, or each copy gets the invalidation.

A status cache in the memory of each copy does not meet the second condition without a message to each copy. Thus this design proposes no status cache in v1. The status read is two reads by primary key on the primary. ADR 0002 permits a cache that a change of status clears. This proposal narrows ADR 0002, and the owner decides it (see "Unresolved questions"). A shared cache, for example in one cache service, can meet the two conditions. M3 can add it if the primary needs it.

### Reads on the primary

A read replica can be some time behind the primary. These reads always go to the primary:

- The status of the account and of the tenant. Test case TC-4-04 requires the new status on the next request.
- Each read in a transaction that also writes.
- Each read by an operator command.

The search route and the document read routes can use a replica. A document that a client uploaded becomes searchable after a job processes it, so the client already waits for that job. A replica that is some seconds behind adds to that wait. The lag of each replica is a metric.

A replica also shows a removed document for some time. After `DeleteDocument`, a search on a replica can find the document until the replica gets the change. The time is the lag of the replica.

### Load on the primary

The status read goes to the primary on each request. At C2 that is approximately 20,000 reads by primary key each second for searches only, and more for the other routes. Thus replicas take the load of the search queries, but the count of requests still sets a load on the primary. This load is small for each request, and the primary must be sized for it. A shared status cache is the next step if a measurement shows that it is necessary.

This design does not decide if v1 uses replicas. It makes sure that the code can use them when M3 decides it.

### Bounds and rejection

Each of these has a bound that the configuration sets:

- The database connection pool of each process, and the time that a request waits for a connection.
- The count of requests that one copy of the API processes at the same time.
- The job queue: the count of jobs that wait. Issue [#5](https://github.com/sravioli/strata/issues/5) designs the queue.
- The size of a request body, and the size of a search result page.

When a request reaches a bound, the API answers with HTTP status 503 and a `Retry-After` header. A full job queue rejects a new upload with the same response. The client then tries again later. No process holds work in memory without a bound.

### Independence of tenant count

This rule comes from the tenancy design, and this design adds the targets. Each target in the ceiling holds for 1 tenant and for 50,000 tenants. Test case TC-2-02 compares the two cases.

### What is not decided here

- Instance sizes, replica counts and the number of copies of each process.
- A limit on the load of one tenant. See "Unresolved questions".

## Alternatives considered

**Design for a forecast, not for the worst case.** For example, 10 % of the accounts active at the same time. This needs less hardware in tests. But the owner does not know the peak, and production is the first place that shows a forecast that is too low. A ceiling with stateless processes costs little in the code. The hardware for the real load is a deployment decision.

**Sticky sessions and state in memory.** A copy keeps the data of its sessions, for example the status of the account. This removes some database reads. But a copy then cannot stop without the loss of sessions, and a new copy does not take load at once. It also makes the status cache of ADR 0002 incorrect between copies.

**All reads on replicas.** This gives the most read capacity. But the status read then can be behind the primary, and TC-4-04 fails. A write followed by a read in the same transaction also fails.

**Queues without a bound.** A request waits until the system can process it. Under overload, memory then increases until a process stops, and each request becomes slow. A rejection with `Retry-After` keeps the accepted requests fast.

## Proof obligations

N/A. This design sets numbers and rules for the system, not a function with a rule. L3 test cases measure the ceiling. L1 and L2 test cases check the rules S1, S2 and S4.

## Impacts

### Infrastructure

- No new service. A read replica is optional, and M3 decides it.
- Metrics: the use and the rejections of each bound (S6), and the wait for a database connection.
- Metrics: the lag of each replica, and the latency of the search route.
- An alert when the count of rejections increases. A rejection shows that a bound is too small or that the load is above the ceiling.
- Rollout: this design is part of the first version. There is no earlier state.

### Security and tenant isolation

- A rejection response contains no data of a tenant, and it is the same for each tenant.
- One tenant can use all capacity and cause rejections for other tenants. This design does not prevent that. See "Unresolved questions".
- A bound on the size of a request body limits the effect of a large upload from a client.
- New input that is not trusted: none.

### Data and migration

N/A. This design adds no table and no column.

### Developer

- Each new pool, queue, buffer or cache needs a bound in the configuration and a metric.
- A developer who adds a read must decide if it goes to the primary. The default is the primary. A read goes to a replica only when the route is in the list of S2.
- L3 test cases need a deployed test environment. Until M3 gives one, they stay `pending`.

## Test cases

L0: unit tests cover the response when a request reaches a bound, and the choice between primary and replica for each kind of read.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-2-01 | L3 | Given the corpus of C5 and 50,000 accounts. When the accounts send 10,000 searches each second for 30 minutes. Then the latency meets C6, and less than 0.1 % of the requests fail. | (not decided) | pending |
| TC-2-02 | L3 | Given the corpus of C5, first in 1 tenant, then in 50,000 tenants. When the load of TC-2-01 runs on each. Then the latency at the 95th percentile of the two runs differs by 10 % or less. | (not decided) | pending |
| TC-2-03 | L3 | Given the system at the load of TC-2-01. When the load increases to 20,000 searches each second for 10 minutes. Then each request that the API does not process gets 503 with `Retry-After`, no process stops, and the memory of each process stays below its limit. When the load goes back to 10,000 each second. Then the latency meets C6 again in 1 minute or less. | (not decided) | pending |
| TC-2-04 | L3 | Given 50,000 accounts and the upload rate of C4 for 1 hour. Then the count of waiting jobs stays below its bound. Then 95 % of the documents become searchable in 5 minutes or less after the upload (proposal, owner decides). | (not decided) | pending |
| TC-2-05 | L2 | Given two copies of the API. When one account sends requests that a load balancer sends to the two copies in turn. Then each request gets the same result as with one copy. When one copy stops. Then the other copy serves each new request. | (not decided) | pending |
| TC-2-06 | L1 | Given a database pool that is full, and a request that waits for a connection. When the wait reaches its time limit. Then the API answers 503 with `Retry-After`, and it does not wait longer. | (not decided) | pending |
| TC-2-07 | L1 | Given a read replica that is behind the primary. When an operator sets an account to not active, and the account sends a request. Then the API rejects the request. | (not decided) | pending |

TC-2-01 to TC-2-04 stay `pending` until M3 gives a deployed test environment. TC-2-01 and TC-2-02 use vector search, so they also need M2.

## Unresolved questions

The owner must answer these questions before this design can be accepted:

1. Are C1 and C2 correct as the ceiling? Issue [#2](https://github.com/sravioli/strata/issues/2) said "50,000 open sessions". With access tokens and stateless processes, Strata has no session. This design replaces "sessions" with "accounts that send requests in the same period".
2. Is the upload rate of C4 correct?
3. Is the corpus of C5 correct?
4. Are the latency targets of C6 correct?
5. Is the target of TC-2-04 correct: 95 % of documents searchable in 5 minutes or less?
6. Does v1 have no status cache? This narrows ADR 0002, which permits a cache that a change of status clears. See "Stateless processes".
7. Must Strata limit the load of one tenant, so that one tenant cannot cause rejections for the other tenants? If yes, is the limit a decision of v1 or of M3?

Implementation resolves these points:

- The value of each bound, and the time limit of the wait for a connection.

These questions belong to later designs:

- Push connections for the state of a document, for example server-sent events. Their count can be up to C1. The design for that channel (M1) must give it a bound.
- Instance sizes, replica counts, hosting and cost (M3).
