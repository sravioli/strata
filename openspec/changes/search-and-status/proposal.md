Issue: #49

# Proposal

## Why

Tech design 0049 builds the walking skeleton in four changes. This is change 4, the last one. After change 3, the worker writes the text parts of a `Ready` revision, but no route reads them, and a client cannot read the status of a revision. This change adds the search route and the status routes of 0047. It also adds `make dev` and the L2 tests, so that one PDF goes from upload to a search result locally (TC-49-01).

## What Changes

- Add `QUERY /v1/search` with the action `Search` (0004, 0049).
  - The body is `{"q": …, "limit": …}`. `q` has 1 to 1,000 characters. `limit` is 1 to 100, and 20 when it is absent. Each other value gives `400`.
  - The body rules of 0047 apply: no `Content-Type` gives `400`, another media type gives `415`, and a body above 16 KB gives `413`.
  - The query is `websearch_to_tsquery('simple', $q)`. The search reads the text parts of the scope of the caller only. It ranks each part with `ts_rank`, keeps the best part of each document, and gives at most `limit` documents, best first.
  - The search has a statement timeout of 2 seconds. A timeout gives `503` with `Retry-After`.
- Add `GET /v1/documents/{document_id}/revisions/{revision}` and the batch route `QUERY /v1/revisions`, with the action `ReadDocument` (0047).
  - They map each internal state to one of the five public statuses of 0047.
  - The response gives `error_class` for `failed`, `no_text` for `ready`, and `updated_at`.
  - A revision that does not exist, or that belongs to a different tenant, gives the same `404`. The batch list omits it.
  - A status that is not final gives `Retry-After`. Each response has `Cache-Control: no-store`. Each response of the batch route has `Accept-Query: application/json`.
- Add one handler type for each `QUERY` path. A request with a different method gets `405` with `Allow: QUERY` (0049).
- Add the limits of status reads of 0047: a token bucket for each pair (account, revision), and one bucket for each account. An empty bucket gives `429` with `Retry-After`. The count of buckets has a bound of 100,000. At the bound, the API removes the bucket that it used least recently (0049).
- Add the bound of 256 requests at the same time in one API process (0002, 0049). A request above the bound gets `503` with `Retry-After` at once (S4 of 0002).
- Add a fourth migration that gives `strata_api` `select` on `text_parts`.
- Add `make dev` (0049): it starts the services, applies the migrations, starts `strata-dev serve`, creates the tenants `acme` and `globex` with their bindings, and starts `strata api` and `strata worker`.
- Add `make l2` and the L2 tests. They run over HTTP against the processes of `make dev`. CI runs `make l2` in a job of its own after the `l1` job.

Not in this change:

- Snippets, pages of results, and ranking other than `ts_rank` ([#73](https://github.com/sravioli/strata/issues/73)). #73 also removes the statement timeout of the search before M2 ends.
- A rate limit for all routes ([#53](https://github.com/sravioli/strata/issues/53)). The buckets of this change are for the status routes only.
- Metrics for each bound ([#61](https://github.com/sravioli/strata/issues/61), M3). This change logs each rejection at a bound.
- Read replicas (0047). The skeleton has no replica.
- The removal of a document ([#69](https://github.com/sravioli/strata/issues/69)), and with it TC-3-12.
- Webhooks ([#52](https://github.com/sravioli/strata/issues/52)).

## Decisions for this change

The owner decided these points on 2026-11-24 ([#49](https://github.com/sravioli/strata/issues/49)):

- **TC-3-12 moves out of change 4.** Its "removes" clause needs the removal route of #69. TC-3-12 stays `approved` in tech design 0003 until that route exists.
- **The L2 tests run with `make l2`, and CI runs them too.** `make l2` starts the processes of `make dev`, runs the L2 test cases over HTTP, and stops the processes. CI runs `make l2` in a job of its own after the `l1` job, with the same nsjail, cgroup and `pdftotext` setup as the `l1` job.

The agent decided these points. The owner reviews them in the pull request:

- **The `Retry-After` of a status.** The setting `STRATA_STATUS_RETRY_AFTER` holds the value in seconds, from 1 to 3,600. The default is 2 (0047). A value out of range stops `strata api` with an error that names the variable. The token buckets of the status routes use the same value.
- **The `Retry-After` of `429`.** The time until the empty bucket gets its next token, rounded up to whole seconds, and at least 1.
- **The `Retry-After` of `503`.** 1 second for the bound of 256 requests and for a search timeout, the same as the end of the wait for a connection.
- **The token buckets have no new dependency.** The API keeps them in one map in memory, with a least-recently-used order, behind one lock.
- **The map of public statuses** is one pure function of the revision, with L0 tests. It reads no job, so a `Parked` job gives `processing` (TC-47-02).
- **`updated_at`** is `status_at` of the revision, in the same format as `expires_at` of a start: RFC 3339, UTC, whole seconds.
- **The search result** gives `revision` and the `location` of the best part. `score` is the `ts_rank` of that part.
- **`make dev` and `make l2`** use one script, `scripts/dev.sh`. With `--background`, the script starts the processes, waits until the API answers, and returns. `make l2` then runs the tests and stops the processes. The script runs the worker in a delegated cgroup with `scripts/in-cgroup.sh`. A second run of the script does not fail on the tenants and the bindings that exist.

## Test cases

This change carries these test cases, each as a scenario with its TC ID:

- 0003: TC-3-10 (L2), TC-3-19 (L2).
- 0004: TC-4-01, TC-4-03, TC-4-09 (L2).
- 0047: TC-47-01 to TC-47-08, TC-47-10, TC-47-11 (L2), TC-47-12 to TC-47-15, TC-47-17.
- 0049: TC-49-01 (L2), TC-49-05, TC-49-07, TC-49-08.

These test cases have a scenario with the marker `PENDING` from change 3. This change adds the search clause and removes the marker:

- TC-49-02: a search returns revision 2 only.
- TC-49-06: a search for a word in the last part returns the page.

These test cases stay `pending` or `approved` after this change:

| Test case | Reason | Issue |
|---|---|---|
| TC-3-12 | It needs the removal route. | [#69](https://github.com/sravioli/strata/issues/69) |
| TC-7-09 | It needs DOCX, PPTX, XLSX and CSV. | [#102](https://github.com/sravioli/strata/issues/102) |

## Capabilities

### New Capabilities

- `document-search`: `QUERY /v1/search`, its body, its query, its rank, its scope and its timeout.
- `revision-status`: the status of one revision, the batch status, the map of public statuses, `Retry-After`, and the `QUERY` handler of the batch route.
- `request-limits`: the bound of 256 requests at the same time in one API process, and the token buckets of the status routes.

### Modified Capabilities

- `document-text`: the search clauses of TC-49-02 and TC-49-06 replace their `PENDING` markers.
- `tenant-isolation`: the grants of `strata_api` get `select` on `text_parts`. An L2 test calls each endpoint with the identifiers of a different tenant (TC-3-19).
- `request-authentication`: an account that is not active gets a rejection on each account action, and the API runs no query on documents (TC-4-03).
- `dev-services`: `make dev`, `make l2`, and the `l2` job of CI (TC-49-01).

## Impact

- `crates/strata/migrations/`: a new migration `V4` with the grant of `select` on `text_parts` to `strata_api`.
- `crates/strata/src/`: the search route, the status routes, the `QUERY` handler, the token buckets, the bound of 256 requests, and the setting `STRATA_STATUS_RETRY_AFTER`.
- `crates/strata-tests/`: L1 tests of the new routes, and the L2 tests behind the feature `l2`.
- `scripts/dev.sh`, `Makefile` (`dev`, `l2`) and `.github/workflows/ci.yml` (the `l2` job).
- `core` does not change, so the Lean translation and the proofs do not change.
- No new dependency.
- No data exists, so no migration of data is necessary.
