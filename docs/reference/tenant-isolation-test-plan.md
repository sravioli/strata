# Tenant-isolation test plan

> Background reading. This file comes from the handoff package of 2026-11-14 (issue [#10](https://github.com/sravioli/strata/issues/10)). It was written before any decision. Where it differs from an accepted ADR in `docs/adr/`, the ADR is correct.

## Test fixtures

Create at least two tenants (A/B), multiple users per tenant, a user who belongs to both tenants, tenant-specific private documents, shared-within-tenant documents and canary text that uniquely identifies each tenant.

## Required negative tests

- User A cannot fetch, mutate, delete, download or enumerate tenant B document IDs.
- Tenant A search (lexical, trigram, semantic and hybrid) never returns tenant B canary documents or snippets.
- A tenant A user cannot access a tenant A document outside their ACL.
- Changing tenant ID in body, path, query, header or message does not bypass validated membership.
- RLS denies when tenant context is missing.
- A connection returned to PgBouncer and reused by another tenant does not retain prior tenant context.
- Runtime application roles cannot bypass RLS, own tables, truncate tenant tables or assume migration roles.
- Composite foreign keys reject cross-tenant child/parent references.
- Views, functions, exports, background workers and administrative endpoints preserve tenant boundaries.
- Blob SAS token cannot list a container, read another object or write outside the target object; token expires as expected.
- SignalR client cannot join another tenant's group.
- Cache keys cannot return results created under another tenant/user/ACL version.
- Deletion and export flows operate only within tenant scope.

## Adversarial/fault tests

- Reuse Service Bus event ID with another tenant/document payload; worker must reject mismatch against authoritative DB state.
- Replay old revision event after a newer revision is ready.
- Interrupt worker between DB commit and Service Bus completion.
- Expire SAS during upload; ensure failed state and orphan cleanup.
- Simulate tenant suspension/offboarding while work is queued.
- Test large tenant activity while small tenants run searches; measure noisy-neighbor impact.

## Acceptance

All negative tests pass in CI/integration environment; security review signs off RLS policy inventory and cloud role assignments. No cross-tenant content, counts, snippets, filenames or metadata may be exposed.
