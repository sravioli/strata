# Distributed Multi-Tenant Document Indexer — Architecture Handoff

> Background reading. This file comes from the handoff package of 2026-11-14 (issue [#10](https://github.com/sravioli/strata/issues/10)). It was written before any decision. Where it differs from an accepted ADR in `docs/adr/`, the ADR is correct.

**Purpose:** architecture review and implementation planning with another engineering agent.  
**Status:** proposal for review; no ADR in this package is Accepted.  
**Date:** 2026-11-14  
**Primary implementation language:** Rust  
**Target platform:** Linux containers on Azure Kubernetes Service (AKS)

---

## 1. Executive summary

Build a cloud-native, event-driven, multi-tenant document indexing and search platform. It must support approximately 30,000 user accounts across multiple tenants, with a capacity plan that can be expanded to 30,000 concurrent users if that is the actual requirement. The user count alone is not a capacity specification: search QPS, tenant size distribution, chunk count, upload rate, document size, embedding model and latency objectives determine the infrastructure required.

The recommended starting point is a **pooled multi-tenant architecture with strong logical isolation**, using a shared PostgreSQL database/schema with mandatory `tenant_id` columns, PostgreSQL Row-Level Security (RLS) as defense in depth, tenant-aware object keys and narrowly scoped upload/download capabilities. Keep an isolation path for large, regulated or noisy tenants: dedicated database/search shards, storage accounts or deployment stamps.

Use PostgreSQL initially for relational metadata, lexical search, trigram matching and vector search. Do not commit to a single global HNSW index without tenant-aware recall and isolation benchmarks. A shared approximate vector index can allow vectors from other tenants to affect candidate recall and query work even when SQL predicates and RLS prevent cross-tenant rows from being returned. Evaluate exact search for small tenant corpora, fixed tenant-bucket partitions, and dedicated indexes for large tenants.

Prefer Azure Blob Storage for original document objects unless SMB filesystem semantics are a real application requirement. Use direct-to-Blob uploads with short-lived, narrowly scoped user-delegation SAS capabilities. Use a transactional outbox to publish durable processing events to Azure Service Bus. Workers must be idempotent because Peek-Lock processing is at-least-once. Publish document revisions atomically only when all required processing/indexing work is complete.

Use Azure SignalR only for live status hints. PostgreSQL/API status remains authoritative. Separate API, extraction, embedding and indexing workloads so they can scale and be resource-limited independently.

Use Lean 4 to specify and prove domain invariants. Keep a deterministic, safe Rust core suitable for Aeneas translation to Lean. Kani and ordinary Rust/property/integration/fault-injection/load tests complement this work. A Lean specification alone does not prove that the distributed Azure services, Tokio orchestration, SQL, native parsers or deployed infrastructure implement the specification; traceability and conformance testing are required.

## 2. Goals and non-goals

### Goals

- Multi-tenant document upload, lifecycle management and secure retrieval.
- PDF, DOCX, XLSX and CSV extraction; OCR for scanned documents where required.
- Chunking, versioned local embedding generation and durable indexing.
- Keyword, typo-tolerant and semantic search, with hybrid ranking (for example Reciprocal Rank Fusion, RRF).
- Real-time best-effort frontend notifications.
- Horizontal scaling of stateless API and independent worker pools.
- Explicit capacity validation for 30,000 accounts and a separately defined concurrent-user envelope.
- Formal specification and proof of selected deterministic business logic.
- Clear operational documentation, runbooks and Architecture Decision Records (ADRs).

### Non-goals for the initial release

- Proving the correctness of Azure, Linux, Kubernetes, PostgreSQL or all Rust async/concurrent behavior in Lean.
- Automatically deploying one database/schema/storage account per tenant.
- Introducing a dedicated search engine before benchmark evidence requires it.
- Promising sub-second completion for every document regardless of size. API acceptance latency and end-to-end indexing SLA are separate objectives.

## 3. Capacity assumptions and sizing model

Treat **30,000 registered users** as the provisional requirement, not as 30,000 concurrent searches. Obtain an explicit answer for:

1. Peak concurrent authenticated sessions.
2. Peak search requests per second and searches per active user per minute.
3. Search mix: lexical, fuzzy, semantic, hybrid, filters and ACL checks.
4. Total tenants and tenant-size distribution (median, p95, largest tenant).
5. Average/maximum documents per tenant, file size, page count and formats.
6. Average chunks per document and expected total chunks.
7. Uploads per second and bytes/day.
8. Embedding dimensions, model, CPU/GPU requirement and model upgrade cadence.
9. Search p50/p95/p99 objectives; indexing completion SLA by document-size class.
10. Availability target, recovery point objective (RPO), recovery time objective (RTO), data residency and retention requirements.

Illustration only: 30,000 concurrent users each searching once every five seconds produces 6,000 search requests/second. That is a materially different system from 30,000 registered users with 500 active users. Do not use this illustration as a production forecast.

A first sizing model should calculate:

- `search_qps = concurrent_searching_users / average_search_interval_seconds`
- `embedding_vectors = total_documents × mean_chunks_per_document`
- `raw_vector_bytes ≈ vector_count × dimensions × 4` for float32 payload, before tuple, metadata and ANN-index overhead.
- ingestion backlog drain time from measured stage throughput, not worker count alone.

Example: 30 million chunks with 1,536-dimensional float32 vectors require roughly 172 GiB for vector payload alone (30,000,000 × 1,536 × 4 bytes), before PostgreSQL row/index/WAL/backup overhead. This makes chunk count and vector dimensions first-class capacity inputs.

## 4. Proposed component architecture

### Frontend

- Authenticates through the selected identity provider.
- Requests upload intents and short-lived upload capabilities from the API.
- Uploads document bytes directly to Blob Storage.
- Calls an upload-completion endpoint.
- Uses search API for queries and SignalR for best-effort status updates.
- Never chooses or asserts its own authoritative `tenant_id`; tenant context is derived from authenticated identity and validated membership.

### Rust API (Axum + Tower)

- Authentication, tenant membership and authorization checks.
- Rate limiting, per-tenant quotas and upload intent management.
- Search API, document lifecycle and status endpoints.
- Upload capability issuance; direct file bytes do not normally pass through API pods.
- Writes document metadata and outbox rows in one PostgreSQL transaction.
- Emits trace/correlation IDs and audit events.

### Azure Blob Storage

- Holds original files and, if needed, immutable processing artifacts.
- Default object key shape: `tenant/{opaque-tenant-id}/document/{opaque-document-id}/revision/{revision-id}/source`.
- Object naming is a namespace convention, not an authorization boundary.
- Upload/download access is issued for a specific object and operation with short expiry; avoid broad list/prefix access.
- Quarantine and scan untrusted uploads before parser workers process them.
- Shared storage account is the default cost/operations model; large or regulated tenants may be moved to separate accounts/deployment stamps.

### Azure Service Bus

- Durable asynchronous work distribution.
- Prefer Peek-Lock for work that must not be lost.
- Stable message IDs, idempotent consumers, explicit settlement, bounded lock renewal and dead-letter handling.
- Separate queues or subscriptions/workload classes for extraction, OCR, embedding and persistence where independent scaling and retry policies are useful.

### Worker pools

1. **Extraction:** fetch object, validate, malware scan result check, parse, detect language, normalize text, chunk.
2. **OCR (optional):** isolated CPU/GPU pool for scanned/image-based documents.
3. **Embedding:** batch embedding generation with pinned model/version and bounded inference concurrency.
4. **Index/persist:** write revision-scoped chunks and embeddings in batches, then atomically publish active revision.
5. **Outbox/notification:** publish pending messages and notification events; do not block API or worker commits on SignalR delivery.

Keep these as separate deployable worker roles, not necessarily separate microservices or repositories. Share a Rust workspace and domain crate; split further only where independent scaling, security boundary or ownership justifies it.

### PostgreSQL + pgvector + pg_trgm

- System of record for tenants, users/memberships, document lifecycle, revisions, chunks, embeddings, ACLs, outbox/inbox and search metadata.
- PostgreSQL FTS (`tsvector`/`tsquery`) for lexical retrieval.
- `pg_trgm` for typo-tolerant similarity and substring/pattern cases.
- `pgvector` for vector retrieval.
- RRF (or another evaluated fusion method) for hybrid results.
- RLS and explicit tenant/ACL predicates on tenant data.
- PgBouncer/managed PgBouncer and bounded connection budgets.
- Read replicas only after load testing and with explicit read-after-write consistency policy.

Important terminology: PostgreSQL's built-in `ts_rank`/`ts_rank_cd` are not BM25. If true BM25 is a requirement, evaluate `pg_search`/ParadeDB and its license, extension availability and managed-service compatibility, or a dedicated search engine. Otherwise describe the initial lexical ranking accurately as PostgreSQL FTS relevance ranking.

### Azure SignalR

- Best-effort `document.indexed` / `document.failed` notifications.
- Notification payloads contain opaque IDs, revision and status only; avoid document text and long-lived storage URLs.
- Server authorizes tenant/user SignalR groups. Never trust a client-supplied group name.
- Client reconnects and fetches authoritative status from API/DB.

### Observability and platform

- AKS, separate node pools where useful (API/general, CPU parsing, embedding/GPU if selected).
- HPA for CPU/memory and KEDA or equivalent event-driven scaling from queue age/backlog.
- OpenTelemetry traces, structured logs, metrics, alerts and audit logging.
- Managed identities, Key Vault, private networking/endpoints, TLS, encryption at rest, image scanning and IaC (Bicep/Terraform).

## 5. Multi-tenancy model (mandatory design area)

### 5.1 Identity and membership model

A user can belong to one or more tenants. Suggested core entities:

- `users(user_id, identity_subject, ...)`
- `tenants(tenant_id, status, plan, region, ...)`
- `tenant_memberships(tenant_id, user_id, role, status, ...)`
- `documents(tenant_id, document_id, owner_user_id, ...)`
- `document_revisions(tenant_id, document_id, revision_id, state, ...)`
- `document_chunks(tenant_id, document_id, revision_id, chunk_id, ...)`
- `document_embeddings(tenant_id, document_id, revision_id, embedding_model_id, ...)`
- `document_acl(tenant_id, document_id, principal_type, principal_id, permission, ...)`

Use opaque IDs. Every tenant-owned row carries `tenant_id`. Add composite uniqueness/foreign-key constraints such as `(tenant_id, document_id)` so a child row cannot reference a document belonging to another tenant. The actual schema should be reviewed for all joins and cascades.

Tenant boundary and within-tenant document permissions are separate concerns. If members in the same tenant do not all have access to every document, search must enforce document/group ACLs as well as tenant scope.

### 5.2 Authorization chain

1. Authenticate principal.
2. Resolve the active tenant from a trusted route/header/session selection, then validate membership and tenant status server-side.
3. Check action-level permission and quota.
4. Begin DB transaction and set transaction-local tenant context.
5. Execute parameterized SQL with tenant predicates; RLS provides defense in depth.
6. Apply document ACL predicate where required.
7. Return only authorized rows and tenant-scoped capabilities.

Do not trust a body/query `tenant_id` without membership validation. Do not rely on frontend filtering.

### 5.3 PostgreSQL RLS

- Enable RLS on every tenant-owned table; create restrictive, reviewed policies for SELECT/INSERT/UPDATE/DELETE.
- Consider `FORCE ROW LEVEL SECURITY` where appropriate so table owners are subject to policies; runtime roles must not own tables and must not have `BYPASSRLS` or superuser privileges.
- Use a separate migration/table-owner role from API and worker runtime roles.
- Set tenant context transaction-locally (for example `SET LOCAL app.tenant_id = ...`) inside an explicit transaction, especially with PgBouncer transaction pooling. Do not rely on session-level `SET` surviving pool behavior.
- On missing context, default-deny.
- Test every table, view, function, materialized view and background job. RLS does not automatically govern every operation (for example `TRUNCATE` is not row-filtered), and table owners/superusers/BYPASSRLS can bypass it.
- Important threat-model limit: a service role allowed to set an arbitrary tenant GUC can impersonate another tenant if that trusted service is compromised. RLS is an excellent query-omission defense, not a substitute for app authorization or a strong independent boundary against compromised service credentials. For stronger isolation, use separate principals/deployments/data stores for the relevant tier.

### 5.4 Tenant-aware search and vector isolation

- Every candidate retriever must run within validated tenant context and include tenant + applicable document ACL filters.
- Cache keys include tenant, user/role or ACL version, query normalization, embedding/index version and filter set. Never share private cached results across tenant contexts.
- `pgvector` warns that a shared approximate index can allow one tenant's vectors to affect another tenant's recall and search work, even if filters prevent unauthorized rows from being returned. Security isolation and retrieval/performance isolation are distinct.
- Evaluate these strategies with realistic tenant distributions:
  - exact vector search after tenant filtering for small tenant corpora;
  - one shared HNSW index with tenant predicate and iterative scans, only if recall/latency are acceptable;
  - fixed hash-bucket partitions/indexes to limit the candidate domain (not equivalent to a dedicated tenant index);
  - dedicated partition/table/index or separate search shard for large/high-isolation tenants.
- Do not create 30,000 partitions by default. Measure planner/catalog/maintenance overhead before per-tenant physical isolation at that count.

### 5.5 Tenant fairness and noisy-neighbor controls

- Per-tenant rate limits, storage quotas, maximum document sizes/pages/chunks, concurrent job limits and embedding budgets.
- Weighted/fair scheduling or admission control so one tenant cannot monopolize all worker slots.
- Per-tenant usage accounting and billing-ready events, without high-cardinality tenant labels in every metrics series.
- Large tenant migration path to dedicated storage/database/search shard/deployment stamp.
- Define retention, export, data deletion and legal hold semantics per tenant.

### 5.6 Storage and realtime isolation

- Blob paths contain opaque tenant/document/revision IDs, but paths alone do not secure objects.
- Issue short-lived, operation-scoped, single-object SAS tokens; prefer user-delegation SAS where the Azure identity design supports it.
- Shared container/account is pooled isolation, not dedicated throughput. Separate containers provide management/access boundaries but still share account capacity; separate accounts/deployment stamps are the stronger performance/isolation tier.
- SignalR group membership is authorized server-side for tenant/user; notification events are tenant scoped and contain no sensitive body data.

## 6. Ingestion and indexing protocol

### 6.1 Upload flow

1. API authenticates user, verifies tenant membership/role, checks quota and creates an `upload_intent`/document revision in `upload_pending` state.
2. API returns a short-lived capability limited to the intended object and operation.
3. Client uploads directly to Blob Storage quarantine/staging object.
4. Client calls completion endpoint; API verifies object existence, content length, hash/checksum, expected object identity and metadata. Do not trust extension or client MIME declaration alone.
5. Scan/validation stage completes.
6. In one PostgreSQL transaction, transition the revision to `queued` and insert an outbox event.
7. Outbox dispatcher sends stable-ID event to Service Bus and marks it published only after confirmed send (with duplicate-safe behavior).

Blob Storage and PostgreSQL do not share a distributed transaction. Use explicit upload states, idempotent completion, orphan cleanup and reconciliation rather than claiming an atomic cross-service write.

### 6.2 Event envelope

Suggested fields:

- `event_id` (stable UUID/idempotency key)
- `event_type` and `schema_version`
- `tenant_id`, `document_id`, `revision_id`
- `blob_object_id` (not a SAS URL or secret)
- `occurred_at`, `correlation_id`, optional `traceparent`

The worker revalidates the document/revision/tenant state from the database before processing. Message contents are not an authorization authority.

### 6.3 Delivery and idempotency

- Peek-Lock means at-least-once delivery. The worker can be restarted or lose a lock and receive the event again.
- Use an inbox/processed-message table with unique `event_id`, or deterministic upsert keys plus state transition checks.
- Ensure each stage is idempotent; include document revision and processing/model version in identities.
- Complete messages only after durable stage completion. Right-size/renew locks for long processing and avoid excessive prefetch.
- Retry transient faults with bounded exponential backoff/jitter. Route poison/permanent failures to dead-letter queue with reason and replay procedure.
- Reconciliation detects stuck revisions, absent messages, orphan blobs, dead-lettered jobs and incomplete indexing.

### 6.4 Revision publication

Write extracted text, chunks and embeddings into an inactive revision/staging generation in batches. Search queries only target the `active_revision_id` or an explicit ready state. After all required index artifacts commit, a short transaction changes the active revision pointer and marks the revision ready. A stale worker must not publish an older revision after a newer revision is active.

### 6.5 Deletion and reprocessing

Deletion is a workflow, not a single SQL `DELETE`: tombstone first; prevent new search results; delete chunks/embeddings/cache entries and source objects according to retention/legal-hold policy; emit audit events; reconcile asynchronous cleanup. Define how replicas, backups and immutable retention interact with a deletion request.

## 7. Search architecture

### 7.1 Candidate retrieval

- Lexical: Postgres FTS with language-specific `tsvector` configurations and weighted fields.
- Fuzzy: `pg_trgm` for misspellings, partial terms and similarity cases; tune thresholds by language/query class.
- Semantic: pgvector using pinned embedding model/dimension and version.
- Hybrid: retrieve tenant-authorized candidates from each method, then fuse ranks (RRF is a reasonable baseline), deduplicate and apply deterministic tie-breaking.
- Optional reranker can be evaluated later; it adds latency/cost and must preserve tenant/ACL boundaries.

### 7.2 Ranking semantics

Use PostgreSQL FTS terminology accurately: `ts_rank`/`ts_rank_cd` are PostgreSQL relevance functions, not BM25. If BM25 is a product requirement, evaluate ParadeDB/`pg_search` (license and Azure extension availability must be checked) or a separate search system. Do not implement a bespoke scorer before building a labeled relevance set.

RRF formula for a candidate document `d`:

`RRF(d) = Σ_i 1 / (k + rank_i(d))`

Specify missing-result handling, rank cutoff, duplicate chunk/document aggregation, tie-break rules and ACL filter placement. The formal Lean model should use exact rational arithmetic where practical; SQL/float conformance must still be differential-tested.

### 7.3 Search quality and performance

Track Recall@K, Precision@K, MRR, nDCG, latency p50/p95/p99, candidate counts, tenant filter selectivity, HNSW recall and index size. Maintain a fixed labeled evaluation set plus anonymized production feedback with an appropriate privacy policy.

### 7.4 Database scaling

- Use bounded pools and PgBouncer; never equate users/websocket connections with Postgres connections.
- Keep total server connection budget across all pods below a measured safe limit.
- Add read replicas when read load justifies them; decide how to handle replica lag and read-after-write search consistency.
- Tune indexes, autovacuum, WAL, `work_mem`, `maintenance_work_mem`, cache and HNSW parameters based on workload.
- Introduce table partitioning/sharding only after measured volume/query plans justify the operational complexity.
- Keep migrations backwards-compatible (expand/contract) across simultaneously running API/worker versions.

## 8. Rust implementation structure

Suggested monorepo/workspace boundary:

```text
/apps/
  api/                    # Axum HTTP API
  worker-extract/         # parsing and chunking
  worker-embed/           # ONNX embedding inference
  worker-index/           # PostgreSQL writes and revision publication
  outbox-dispatcher/      # Service Bus and SignalR event publishing
/crates/
  domain-core/            # deterministic, mostly pure, safe Rust logic
  application/            # use cases and ports
  persistence-postgres/   # SQLx or chosen DB adapter
  messaging-servicebus/   # Service Bus adapter
  storage-blob/           # Blob Storage adapter
  realtime-signalr/       # notification adapter
  parser-pdf/
  parser-office/
  parser-csv/
  search/                 # candidate retrieval/fusion abstractions
/formal/
  lean/                   # Lean project and proofs
  aeneas/                 # Charon/Aeneas inputs and generated artifacts
/docs/
  architecture/
  adr/
  security/
  testing/
  runbooks/
```

Keep domain rules independent from Azure SDKs, SQL connections and Tokio runtime where practical. Async adapters call deterministic domain functions. `spawn_blocking` or dedicated bounded pools should handle blocking/CPU-bound libraries; do not run large parsing/inference tasks on Tokio async executor threads without bounds.

Do not assume `tikv-jemallocator` is automatically beneficial. Benchmark allocator choice per worker class using RSS, fragmentation, tail latency and throughput under representative parser/embedding loads. Keep it as a measured optimization, not a correctness prerequisite.

## 9. Formal verification strategy

### 9.1 Verification objective

Prove selected domain properties and maintain evidence connecting requirements to Lean theorems, actual Rust code and integration tests. Do not describe the entire deployed distributed service as formally verified.

### 9.2 Candidate Lean model/invariants

- `Searchable(document) → ActiveRevision(document) = revision ∧ RevisionReady(revision)`.
- Every chunk/embedding belongs to exactly one `(tenant_id, document_id, revision_id)` and its parent document has the same tenant.
- For any search request with tenant `t`, every returned result has tenant `t` and satisfies the defined document ACL predicate.
- A deleted/tombstoned revision cannot become searchable again.
- A stale revision cannot replace a newer active revision.
- Replaying the same event does not alter the logical final state after first successful application.
- Chunking preserves required text coverage/order and obeys maximum chunk/token bounds; overlap semantics must be specified.
- RRF satisfies specified ordering/monotonicity properties under its mathematical model.
- Quota/admission-control transitions never exceed configured bounds.

### 9.3 Lean/Aeneas/Kani roles

- **Lean 4:** abstract state machine, invariants, algorithms and theorem proofs.
- **Aeneas + Charon:** translate the supported safe/sequential Rust subset to a functional representation that can be proved in Lean. This is a good fit for `domain-core`; it is not a proof path for arbitrary Tokio concurrency, unsafe code or native SDK behavior.
- **Kani:** bounded model checking for selected Rust functions, assertions, panic/overflow and bounded behaviors; it complements proofs, not a substitute for unbounded theorem proofs.
- **Property-based tests:** broad randomized tests and cross-checks between Lean reference functions and Rust outputs.
- **Integration/fault-injection tests:** SQL transactions, RLS, Service Bus redelivery, blob failures, process restarts, lock expiry and replica lag.
- **Load tests:** performance evidence; formal proofs do not establish latency or throughput.

### 9.4 Proof discipline

- Pin Rust, Lean, Mathlib (if used), Charon and Aeneas versions.
- Keep generated translation reproducible in CI.
- No `sorry`/admitted theorem in a release proof target; explicitly audit axioms and assumptions.
- Store a traceability matrix: requirement → invariant → Lean theorem → Rust implementation function → conformance/property test.
- Treat SQL policy correctness as a separately reviewed and tested artifact; Lean proofs of an abstract tenant model do not prove PostgreSQL RLS configuration.
- Make all assumptions explicit (e.g. database transaction atomicity, storage object identity, message broker delivery behavior).

## 10. Security baseline

- AuthN via a selected OIDC/Microsoft Entra-compatible identity provider; AuthZ based on tenant membership and document ACLs.
- Runtime managed identities; no long-lived cloud credentials in images or repository.
- Private networking/private endpoints where supported; TLS in transit and encryption at rest.
- Separate runtime, migration and operational database roles; least privilege; no superuser/BYPASSRLS in app pods.
- Strict file size/page/ZIP expansion/CPU/memory/time limits; MIME sniffing and file signature validation; malware scanning; parser sandboxing; read-only root filesystem and non-root containers.
- Egress restrictions for parsing workers where feasible; keep parser and native-library dependencies patched.
- Rate limits, quotas, abuse detection, audit trails and secret rotation.
- Tenant-scoped cache keys, download capabilities, SignalR groups and exports.
- Threat model covers IDOR, tenant-context confusion, RLS bypass, stale SAS, blob enumeration, malicious documents, ZIP bombs, parser CVEs, SSRF, message spoofing/replay, log leakage and noisy neighbors.

## 11. Reliability and operations

- Define SLOs separately: API response/acceptance, search latency, indexing completion by file-size class, availability and notification delay.
- Initial *candidate* targets for discussion: search p95 <250 ms and p99 <1 s for a specified query mix and dataset; not guaranteed until benchmarked. Define the indexing SLA by pages/bytes/chunks, not a single universal sub-second target.
- Monitor queue depth and oldest message age, stage throughput, retry/dead-letter counts, parser memory/timeouts, DB pool wait, query p95/p99, replica lag, HNSW recall, storage throttling, SignalR failures and per-tenant quota consumption.
- Do not put 30,000 tenant IDs into high-cardinality metric labels. Use aggregate metrics and sampled/log-based tenant diagnostics.
- Backups/PITR, restore drills, cross-region strategy, RPO/RTO, schema migration rollback/forward policy, runbooks and incident ownership are required before production.
- Test graceful termination: stop accepting work, finish or release in-flight messages, checkpoint/commit, then exit.
- Define DR behavior for storage, database, Service Bus and SignalR separately; region failover does not automatically make a multi-service workflow atomic.

## 12. Suggested repository documentation structure

This section listed the files of the handoff package. The triage in issue [#10](https://github.com/sravioli/strata/issues/10) kept only the files in `docs/reference/`, and the accepted ADRs in `docs/adr/` replace the draft ADRs of the package.

## 13. Decisions requiring agent review

1. Is the target 30,000 registered users, monthly active users, or simultaneous authenticated/searching users?
2. How many tenants? Can users belong to multiple tenants? What roles and document ACL model are required?
3. What is the expected tenant size distribution and largest tenant?
4. Are tenant-level isolation/performance requirements satisfied by pooled PostgreSQL RLS, or are dedicated deployment tiers required for particular customers?
5. What is the vector-index physical strategy for 30,000 tenants: exact search for small corpora, fixed hash buckets, and dedicated large-tenant indexes? What benchmark chooses the thresholds?
6. Is true BM25 a product requirement, or is PostgreSQL FTS relevance sufficient?
7. Are scanned PDFs/OCR in scope for the first release?
8. Is Azure Blob Storage acceptable, or is SMB/Azure Files a hard compatibility constraint?
9. Which identity provider, regions, residency rules, encryption/key ownership and compliance regime apply?
10. What are the required search SLO, indexing SLA, uptime, RPO and RTO?
11. Which embedding model, dimensions, languages, license and upgrade/re-index policy are acceptable?
12. Is 30,000 concurrent websocket clients or 30,000 concurrent search users expected?
13. Which Azure Database for PostgreSQL version/tier/extension matrix is approved for the target region?
14. How are user deletion, tenant offboarding, export, legal hold and backup retention expected to work?

## 14. Reference links

The following upstream documentation should be checked again during implementation because product limits, versions and extension availability change:

- PostgreSQL Row-Level Security: https://www.postgresql.org/docs/current/ddl-rowsecurity.html
- pgvector (filtering, iterative scans and multi-tenancy): https://github.com/pgvector/pgvector
- Azure multi-tenant Blob Storage architecture: https://learn.microsoft.com/azure/architecture/guide/multitenant/service/storage
- Azure SAS overview and user-delegation guidance: https://learn.microsoft.com/azure/storage/common/storage-sas-overview
- Azure Service Bus duplicate delivery and loss guidance: https://learn.microsoft.com/azure/service-bus-messaging/service-bus-message-loss-and-duplicates
- Azure Service Bus lock/settlement semantics: https://learn.microsoft.com/azure/service-bus-messaging/message-transfers-locks-settlement
- Azure Database for PostgreSQL PgBouncer: https://learn.microsoft.com/azure/postgresql/connectivity/concepts-pgbouncer
- Lean 4 reference: https://lean-lang.org/doc/reference/latest
- Aeneas project and Rust subset: https://aeneasverif.github.io/projects/
- Aeneas getting started: https://github.com/AeneasVerif/aeneas/blob/main/documentation/getting-started.md
- Kani getting started and limitations: https://model-checking.github.io/kani/getting-started.html

---

**Review instruction for the next agent:** Treat this as a proposal. Challenge assumptions, identify hidden failure modes, make any required decisions explicit in ADRs, and do not convert candidate SLOs or sizing examples into guarantees without benchmark evidence.
