---
status: "accepted"
date: 2026-11-22
decision-makers: sravioli
tech-design: docs/tech-design/0049-walking-skeleton.md
---

# The walking skeleton extracts PDF in the sandbox, indexes text in PostgreSQL, and proves the lifecycle of a revision

## Context and Problem Statement

The accepted designs decide each part of Strata alone, and no design connects them. What is the shortest path from upload to a tenant-scoped lexical search result that runs locally? What are the internal states of a revision? The issue is [#49](https://github.com/sravioli/strata/issues/49).

## Considered Options

* PDF only with `pdftotext` in the real sandbox, and PostgreSQL full-text search
* All formats and both PDF parsers in the skeleton
* Extraction in the worker process first, and the sandbox later
* A separate search engine, for example ParadeDB or Tantivy

## Decision Outcome

Chosen option: "PDF only with `pdftotext` in the real sandbox, and PostgreSQL full-text search". It goes through each component, the security boundary of ADR 0006 included, with no new service.

The decision has these parts:

* A revision has five internal states: `AwaitingUpload`, `Uploaded`, `Ready`, `Failed` and `Superseded`, one for each public status of ADR 0009. The retry mark, `no_text`, `error_class` and the time of the status are columns.
* The proved pure functions `revision_step` and `may_extract` in `core` hold the transitions. A final state never changes, except from `Ready` to `Superseded`.
* One transaction ends a job. It locks the document, writes the text parts, sets `Ready`, sets older revisions to `Superseded` and removes their text parts.
* PostgreSQL 19 full-text search with the configuration `simple`. One row for each page, with parts of at most 64 KB, and a GIN index on `(tenant_id, tsv)`.
* `QUERY /v1/search` returns one result for each document of the tenant of the caller, with no snippets.
* The upload routes are under `/v1/documents`.
* Crates: `core`, `strata` (`api`, `worker`, `admin`), `strata-extract`, `strata-dev`. Libraries: axum, tokio, tokio-postgres with deadpool-postgres, refinery for the migrations, `object_store`, `jsonwebtoken`. Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): `rusty-s3` replaces `object_store`, because `object_store` adds a second crypto backend and fails `make deny` (owner, 2026-11-22). The benchmark ([#100](https://github.com/sravioli/strata/issues/100)) found tokio-postgres better than sqlx by more than 20 % on the tenant transaction and on the batch status read (2026-11-22).
* Statements that do not need an earlier result go to PostgreSQL together, in one round trip.
* Identifiers are `uuidv7()` UUIDs, and `core` uses `u128`. PgBouncer runs in transaction mode locally too.
* Each step reads or writes its rows with one statement.

### Consequences

* Good, because each component runs end to end before the work grows wider.
* Good, because the search needs no new service, and the tenant policy of ADR 0001 also covers it.
* Good, because the sticky rule of the final states is proved, not only tested.
* Bad, because the skeleton extracts only PDF until [#102](https://github.com/sravioli/strata/issues/102).
* Bad, because the cost of a search grows with the matches in one tenant until [#73](https://github.com/sravioli/strata/issues/73).
* Bad, because revisions in `AwaitingUpload` are not cleaned up until [#69](https://github.com/sravioli/strata/issues/69) gives the cleanup a principal.
* Bad, because development needs Linux, and PostgreSQL 19 is new.
