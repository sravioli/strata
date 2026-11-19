# Initial threat model

> Background reading. This file comes from the handoff package of 2026-11-14 (issue [#10](https://github.com/sravioli/strata/issues/10)). It was written before any decision. Where it differs from an accepted ADR in `docs/adr/`, the ADR is correct.

This is a starter threat model and must be refined with the selected identity provider, compliance regime and deployment topology.

| Threat | Example | Required controls |
|---|---|---|
| Cross-tenant data exposure | Missing tenant predicate or wrong tenant context | Membership validation, RLS, composite FKs, explicit predicates, adversarial tests |
| Within-tenant unauthorized access | User searches another group's private document | Document ACLs enforced in every retrieval branch and cache |
| Tenant context confusion | Header/body tenant ID trusted without membership check | Derive from authenticated principal and validated membership |
| RLS bypass | Runtime role owns table or has BYPASSRLS | Separate migration role, least privilege, FORCE RLS review |
| Blob exposure | Predictable object key or broad SAS/list token | Object-scoped short-lived SAS, private network, no account keys |
| Malicious upload | ZIP bomb, parser exploit, malformed PDF | Malware scan, signature validation, resource limits, sandbox, patching |
| SSRF/data exfiltration | Parser follows external references or makes outbound calls | Restricted egress, safe parser config, network policy |
| Queue spoof/replay | Forged or duplicated processing event | Broker auth, stable IDs, idempotent handlers, DB revalidation |
| Stale revision publication | Old worker finishes after newer document revision | Revision compare-and-swap and publication transaction |
| Cache leakage | Cache omits tenant or ACL version | Tenant/user/ACL-aware keys and cache invalidation |
| SignalR group abuse | Client subscribes to arbitrary tenant group | Server-side authorization and opaque group mapping |
| Noisy neighbor | One tenant monopolizes OCR/embedding workers | Quotas, fair scheduling and per-tenant concurrency limits |
| Sensitive logs | Document text, SAS URLs or PII in traces | Data minimization, redaction, retention and access controls |

Security review should include API, DB, Blob Storage, Service Bus, SignalR, Kubernetes identities/network policy, CI/CD and backups.
