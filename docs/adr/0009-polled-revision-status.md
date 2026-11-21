---
status: "accepted"
date: 2026-11-21
decision-makers: sravioli
tech-design: docs/tech-design/0047-document-readiness.md
---

# A client polls the public status of a revision, and webhooks come later

## Context and Problem Statement

A worker processes a revision in a job after the upload, and the time is not known. How does a client learn that a revision is ready for search? The clients are product backends, and v1 has no message bus. The issue is [#47](https://github.com/sravioli/strata/issues/47).

## Considered Options

* A status resource that the client polls, and webhooks later
* Webhooks in v1
* Server-sent events
* Show the internal states of a revision to clients

## Decision Outcome

Chosen option: "A status resource that the client polls, and webhooks later". The walking skeleton needs the shortest path from upload to search. Webhooks need an outbox, signatures, retries and protection against server-side request forgery. A webhook is also not reliable alone, so the status resource must exist in all options.

The decision has these parts:

* The API shows five public statuses: `uploading`, `processing`, `ready`, `failed` and `superseded`. A map from the internal states gives them, so a change of the revision lifecycle does not break clients.
* `ready`, `failed` and `superseded` are final. A final status does not change, except from `ready` to `superseded`. The revision stores its final status.
* When a revision becomes ready, each older revision of the same document that is not `failed` becomes `superseded`. A `superseded` revision gets no extraction.
* `failed` shows an error class from a fixed list. `ExtractionCrash`, `ExtractionTimeout` and `InvalidOutput` retry one time first (ADR 0006). A `Parked` job shows as `processing`.
* A `ready` revision with no text shows `no_text` set to `true`.
* `GET /v1/documents/{document_id}/revisions/{revision}` reads one revision. `QUERY /v1/revisions` (RFC 10008) reads 1 to 100 revisions.
* A response for a status that is not final has `Retry-After`. Each API instance limits the reads of each revision and of each account with token buckets in memory, and answers `429` above the limit.
* The webhook contract is fixed now: the events `revision.ready`, `revision.failed` and `revision.superseded`, a payload with no content, Standard Webhooks signatures, and delivery at least once. Webhooks come in [#52](https://github.com/sravioli/strata/issues/52).

### Consequences

* Good, because the status resource needs no new service and no new storage.
* Good, because Strata controls the polling load with `Retry-After`, with no new version of the API.
* Good, because the webhooks of [#52](https://github.com/sravioli/strata/issues/52) do not change the API of v1.
* Bad, because a client polls. A notification is faster for one waiting client.
* Bad, because a `ready` revision can become `superseded` with no notice in v1.
* Bad, because the limit is not exact with more than one instance.
* Bad, because a proxy in front of the API must forward the `QUERY` method.
* Not decided: the internal state names of the revision lifecycle, in the design of [#49](https://github.com/sravioli/strata/issues/49).
