---
status: "accepted"
date: 2026-11-18
decision-makers: sravioli
tech-design: docs/tech-design/0002-capacity-ceiling.md
---

# A derived peak as the design ceiling, and stateless processes that scale with copies

## Context and Problem Statement

Strata has approximately 50,000 registered accounts across all tenants, and the peak load is not known. Which load must the system be able to reach, and which rules must the code obey to reach it? L3 test cases need target numbers. The issue is [#2](https://github.com/sravioli/strata/issues/2).

## Considered Options

* A peak derived from the registered accounts, with stateless processes
* All 50,000 accounts concurrent
* Sticky sessions and state in memory
* All reads on read replicas
* Queues without a bound

## Decision Outcome

Chosen option: "A peak derived from the registered accounts, with stateless processes", because it gives test targets with a margin. More copies of a process then give more capacity.

The decision has these parts:

* The design ceiling for one deployment: 10,000 concurrent accounts, 1,000 searches each second and 10,000 uploads each hour. The test corpus has 5,000,000 documents, and one tenant can hold half of them.
* At the ceiling, 95 % of searches take 500 ms or less, and 99 % take 1 s or less. 95 % of uploaded documents become searchable in 5 minutes or less.
* Each target holds for 1 tenant and for 50,000 tenants.
* Each process is stateless. Each copy can serve each request.
* Search and document reads can use a read replica. The status read, reads in a transaction that writes, and operator reads use the primary.
* v1 has no status cache. This narrows ADR 0002, which permits a cache that a change of status clears.
* Each pool, queue, buffer and cache has a bound and a metric. At a bound, the API answers 503 with `Retry-After` in a bounded time.
* Strata does not limit the load of one tenant. It adds copies behind a load balancer.

### Consequences

* Good, because a new copy of a process takes load at once, and a copy can stop with no loss of sessions.
* Good, because a rejection keeps the accepted requests fast under overload.
* Good, because the L3 test cases have numbers to cite.
* Bad, because each request reads the status on the primary, so request volume still sets a load on the primary.
* Bad, because a search on a replica can find a removed document for the time of the replica lag.
* Bad, because one tenant can use all capacity and cause rejections for other tenants.
* Not decided: instance sizes, replica counts, hosting and cost (M3).
