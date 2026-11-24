# Tasks

## 1. Grant and setting

- [x] 1.1 Add the migration `V4` that grants `select` on `text_parts` to `strata_api`. Update the list of grants that the L1 test of the catalog reads. Verify: the scenarios "No privilege beyond the list" and "The API reads text parts and cannot change them" pass.
- [x] 1.2 Add the setting `STRATA_STATUS_RETRY_AFTER` to `strata api`: whole seconds from 1 to 3,600, and 2 when it is absent. A value out of range stops `strata api` with an error that names the variable. Verify: a test of the binary in `crates/strata/tests/cli.rs` gives the error for 0, 3601 and `x`.

## 2. The QUERY handler

- [x] 2.1 Add one handler type for a `QUERY` path. It answers each other method with `405` and `Allow: QUERY` before it reads the token or the body. It checks the body rules of 0047 with the code of the start body: no `Content-Type` gives `400`, a media type other than `application/json` gives `415`, and a body above 16,384 bytes gives `413`. Verify: the L1 test TC-49-08 passes for `/v1/search`, and the scenario "A GET to the batch path" passes. Remove the pending marker of TC-49-08.

## 3. Status routes

- [x] 3.1 Add the map of public statuses as one pure function of the revision, and the JSON of one status: `document_id`, `revision`, `state`, `updated_at` in RFC 3339 UTC with whole seconds, `error_class` for `failed` only, and `no_text` for `ready` only. Verify: L0 tests of each internal state and of each optional field pass.
- [x] 3.2 Add `GET /v1/documents/{document_id}/revisions/{revision}` with the action `ReadDocument`. Read the revision in the tenant transaction of the decision. A revision that does not exist, of a different tenant, or with an identifier that is not valid gives the same `404`. Add `Retry-After` for a status that is not final, and `Cache-Control: no-store` on each response. Verify: the L1 tests TC-47-01, TC-47-02, TC-47-03, TC-47-04, TC-47-10 and TC-47-13, and the scenarios "A ready revision with no text" and "The configured value", pass. Remove their pending markers.
- [x] 3.3 Add `QUERY /v1/revisions` with the action `ReadDocument`, on the handler type of 2.1. Parse 1 to 100 entries, count a repeated entry one time, and read the revisions with one query in the tenant transaction. Omit each revision that the query does not find. Add `Accept-Query: application/json` to each response of the route, `Retry-After` when one entry is not final, and `Cache-Control: no-store`. Verify: the L1 tests TC-47-06, TC-47-07, TC-47-08 and TC-47-15, and the scenario "A repeated entry", pass. Add L0 tests of the parse of the batch body. Remove their pending markers.

## 4. Limits

- [x] 4.1 Add the token buckets of 0047 to the state of the router: one bucket for each pair (account, revision) with 3 tokens and 1 token for each `Retry-After` interval, and one bucket for each account with 20 tokens and 10 tokens each second. A status request takes its tokens only when each bucket has one. Else it gets `429` with `Retry-After`: the time until the next token, rounded up to whole seconds, and at least 1. Keep at most 100,000 buckets, and remove the bucket that the API used least recently at the bound. The bound and the time source are one value, so a test can set them. Verify: L0 tests of the bucket and of the bound, and the L1 tests TC-47-05, TC-47-12 and TC-47-14, and the scenarios "A batch with one empty bucket" and "The bucket used least recently goes first", pass. Remove their pending markers.
- [x] 4.2 Add the bound of 256 requests at the same time to the router of `strata api`. A request above the bound gets `503` with `Retry-After: 1` and no body at once, and the API logs the rejection. The bound is one value, so a test can set it. Verify: the scenario "A request above the bound" passes as an L1 test.

## 5. Search

- [x] 5.1 Add `QUERY /v1/search` with the action `Search`, on the handler type of 2.1. Check `q` (1 to 1,000 characters) and `limit` (1 to 100, 20 when it is absent). Read the text parts of the scope of the caller with `websearch_to_tsquery('simple', $q)` and `ts_rank`, keep the best part of each document, and give at most `limit` results, best first, with `document_id`, `revision`, `score` and `location`. Follow the pipelining rule of 0049. Verify: the L1 tests TC-49-05, TC-49-07, TC-4-01 and TC-47-17, the search clauses of TC-49-02 and TC-49-06, and the scenarios "A body with the default limit", "A body of another media type", "The page of the best match" and "A query that PostgreSQL cannot parse as tsquery syntax" pass. Remove the pending markers of these test cases.
- [x] 5.2 Add the statement timeout of 2 seconds to the transaction of the search. A timeout gives `503` with `Retry-After: 1` and no body, and the API logs it with no text of the query. The timeout is one value, so a test can set it. Verify: the scenario "A search at the time limit" passes as an L1 test.

## 6. Authentication

- [x] 6.1 Verify that an account that is not active gets the rejection of `request-authentication` on each account action: the start of an upload, the finish, the status of one revision, the batch status and the search, and that the API runs no query on documents. Verify: the L1 test TC-4-03 passes, and the check of TC-3-18 finds a test with the identifiers of a different tenant for each new route. Remove the pending marker of TC-4-03.

## 7. make dev and the L2 tests

- [x] 7.1 Add `scripts/dev.sh` and `make dev`, with the steps of the requirement "One command starts the walking skeleton". Start the services with `$(COMPOSE)`, run `strata migrate` as `strata_owner`, start `strata-dev serve`, and create the tenants, the issuer and the bindings with `strata admin` (a second run skips each one that exists). Start `strata api` and `strata worker`, the worker in `scripts/in-cgroup.sh`. With `--background`, the script waits until the API answers and returns. Ctrl-C, or `scripts/dev.sh --stop`, stops each process that it started. Verify: the scenarios "The first run" and "A second run" pass on the laptop.
- [x] 7.2 Add the L2 tests to `crates/strata-tests` behind the feature `l2`, and `make l2`. The tests get tokens from the key file of `strata-dev`, and call the API over HTTP. `make l2` starts `scripts/dev.sh --background`, runs the tests, and stops the processes, also when a test fails. Verify: the L2 tests TC-49-01, TC-47-11, TC-3-10, TC-3-19 and TC-4-09, and the scenario "The processes stop after a failed test", pass. Remove their pending markers.
- [x] 7.3 Add the `l2` job to `.github/workflows/ci.yml`. It needs the `l1` job, has the same setup of the services, nsjail, the delegated cgroup and `poppler-utils`, and runs `make l2`. Add it to the jobs that `ci-ok` needs. Verify: the scenario "The l2 job runs after l1" passes on the pull request.

## 8. Integration and gates

- [ ] 8.1 Run `make`, `make l1` and `make l2`. Verify: each target passes, the coverage of `CONSTRAINTS.md` is 90 % or more, and `python3 scripts/check-test-cases.py --report` shows each scenario of this change as `implemented`. TC-3-12 and TC-7-09 stay as they are. If the coverage is below 90 %, add tests. Do not lower the number.
- [ ] 8.2 Update the implementation-point lists in the comments of #49. Tick the bound on token buckets, the route for `QUERY`, and the bounds of change 4. Verify: each point of #2 and #47 that change 4 owns is ticked.
