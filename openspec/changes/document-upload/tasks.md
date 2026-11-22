# Tasks

## 1. Dependencies and settings

- [x] 1.1 Add `rusty-s3` at the exact version 0.12.0, with no default features and the feature `rustcrypto` (owner, 2026-11-22). Verify: `cargo build --workspace --locked` and `make deny` pass, and `cargo tree -i aws-lc-rs` finds no crate. If `make deny` fails, stop and tell the owner.
- [x] 1.2 Read the six storage variables of the `document-upload` spec into the settings of the storage client. Read no other `AWS_*` variable. A missing `AWS_ACCESS_KEY_ID` or `AWS_SECRET_ACCESS_KEY` gives an error that names the variable. An `http://` endpoint with no `AWS_ALLOW_HTTP=true` gives an error that names `AWS_ALLOW_HTTP`. Add the settings of Garage to the environment of the L1 tests and of the `l1` job of CI, from the development values in `compose.yaml`. Verify: the scenarios "No secret key" and "An http endpoint needs AWS_ALLOW_HTTP" pass in L1 tests of the binary, and `make secrets` passes with no new entry in `.betterleaksignore`.

## 2. Revision state in core

- [x] 2.1 Add `crates/core/src/revision.rs` with the states, the classes, the events, `revision_step` and `may_extract` of the `revision-lifecycle` spec. Use only the proved subset of 0009: enums, structs, `match` and comparison. Add L0 tests for each pair of a state and an event, with and without the retry mark. Verify: `cargo test -p strata-core` passes.
- [x] 2.2 Run `scripts/translate.sh`. Add `lean/Strata/Model/Revision.lean` with the model of the table, and `lean/Strata/Proofs/Revision.lean` with the five theorems of 0049, `revision_step_refines` and `may_extract_refines`. The statements are the statements of the table of 0049. Choose Rust types that let the Lean statements of 0049 type-check as written. The Lean text can change for a type, for example `some c` for an `Option` field, when its meaning stays the same. If the meaning of a statement must change, stop and tell the owner. Verify: `make verify` passes with no `sorry`, and `python3 scripts/proof-obligations.py` finds each theorem.

## 3. Schema

- [x] 3.1 Add `crates/strata/migrations/V2__documents_and_jobs.sql`:
  - `documents` and `revisions` with the columns of 0049, plus `slot_id` on `revisions`. Each follows the three rules of 0003 and has the policy of 0003. The foreign key of `revisions` is `(tenant_id, document_id)`. Check constraints hold the list of states and the list of classes. A class is not null only for `Failed`.
  - `upload_slots` with `slot_id` (a version 4 UUID, the primary key) and `created_at`.
  - `jobs` with the fields and the three indexes of the `job-queue` spec, with check constraints for `kind`, `state` and `last_error`.
  - The grants of the `tenant-isolation` spec.

  Verify: `make sql` passes, and `strata migrate` applies V2 on a database with V1.
- [x] 3.2 Add `jobs` and `upload_slots` to the one list of global tables in the test code. Extend the test of the list of grants to the column privileges. Add the L1 tests TC-3-04 and TC-5-08, and the scenarios of the `tenant-isolation` and `job-queue` specs for the check constraints and for `upload_slots`. Verify: the tests pass, and TC-3-06, TC-3-07 and the grant test still pass. TC-5-08 counts the rows of a global table, so it runs in a new database, with the method of TC-3-13 in change 1. Remove the pending markers of TC-3-04 and TC-5-08.

## 4. Storage

- [x] 4.1 Add the storage module of `strata`. It holds the key of 0006 as a pure function, the signature of a PUT URL with an expiry of 15 minutes, the read of the size of an object, and the removal of an object. `rusty-s3` signs each request. The HTTP client of `strata` sends the HEAD and the DELETE with a signed URL that expires after 60 seconds. A removal of a key that does not exist is not an error. Add L0 tests for the key. Verify: `cargo test -p strata` passes.
- [x] 4.2 Before each other L1 test that uses Garage, add the L1 tests TC-6-01 and TC-6-02, and the scenario "A slow transfer that starts in time completes". The slow transfer uses an expiry of a few seconds and a body that stops before the expiry and continues after it. If Garage refuses a signed URL of `rusty-s3`, or refuses the slow transfer, stop and tell the owner. Verify: the tests pass. Remove the pending markers of TC-6-01 and TC-6-02.

## 5. Upload routes

- [x] 5.1 Add the check of the start body as a pure function: the fields, the limits and the answers of the `document-upload` spec. Add the body rules for `Content-Type` and for 16,384 bytes. Add L0 tests for each limit, at the limit and one past it. Verify: `cargo test -p strata` passes.
- [x] 5.2 Add `POST /v1/documents` and `POST /v1/documents/{document_id}/revisions`. Each takes a `Caller`, gets a scope from `authorized_scope(principal, UploadDocument)`, and opens the tenant transaction from it. A new revision number comes from the `update … returning last_revision` of 0049. The start inserts the revision with a new `slot_id` and `status_at`, and the row of `upload_slots`, in one transaction. It signs the URL after the commit. Follow the batching rule and the pipelining rule of 0049. Verify: the scenarios of the requirement "The start of an upload creates a revision and a signed URL" pass as L1 tests.
- [x] 5.3 Add the routes to the router of `strata api`, with the storage client in the state of the router. Verify: `strata api` starts against the services of `compose.yaml`, and a start with a token of `strata-dev` answers `201`.
- [x] 5.4 Add the L1 test TC-49-04 with two starts at the same time on two connections. Verify: the test passes, and it fails when the start reads `last_revision` with no row lock. Remove the pending marker of TC-49-04.
- [x] 5.5 Add the two bounds to the start: one statement counts the `Waiting` jobs and the rows of `upload_slots` from the last 2 hours, before each write. At a bound, answer `503` with `Retry-After: 60`, and log the name of the bound with no tenant content. Make the two bounds parameters of the router state, so that a test can set a small bound. These tests count the rows of global tables, so they run in a new database. Verify: the L1 test TC-5-06 and the scenarios of the requirement "The start stops at the bounds" pass. Remove the pending marker of TC-5-06.
- [x] 5.6 Add the finish route with the answers of the `document-upload` spec. It locks the revision with `select … for update`, reads the size of the object, and writes the result of `revision_step` with `Finish` or `FinishTooLarge`. Only the change from `AwaitingUpload` to `Uploaded` inserts the job. The insert has no `returning` and no `on conflict`, because `strata_api` can read only `state`. A change from `AwaitingUpload` sets `status_at` and removes the row of `upload_slots` by its `slot_id`. Verify: the L1 tests TC-5-01, TC-6-03, TC-6-04, TC-6-05, TC-47-09 and TC-49-09, and the scenario of a revision of a different tenant, pass. Remove their pending markers.

## 6. Cleanup

- [x] 6.1 Add the cleanup function of `Done` jobs. It connects as `strata_worker` and removes at most 1,000 rows in each batch. Change 3 calls it from the loop of the worker. The tests run in a new database, because other tests write `jobs`. Verify: the L1 test TC-5-10 and the scenario "Jobs in other states stay" pass. Remove the pending marker of TC-5-10.
- [x] 6.2 Add the task of `strata api` that removes the rows of `upload_slots` older than 2 hours, each 60 seconds, at most 1,000 rows in each batch. A failure of one run goes to the log, and the next run continues. The task gets its own handle of the pool in `api::serve`. Verify: the scenario "Old rows go and new rows stay" passes as an L1 test that calls one run in a new database.

## 7. Route checks

- [x] 7.1 Add the check TC-3-18. An L1 test reads each `.route(…)` call of the router of `strata api` from the source, and each marker of a cross-tenant test from the tests. It fails when a route has no marker. Add the cross-tenant test of each of the three upload routes. For `POST /v1/documents`, the test checks that an account of B cannot start a revision of the new document of A. Verify: the check passes. It fails when a route has no test. Remove the pending marker of TC-3-18.
- [x] 7.2 Add `compile_fail` doc tests for TC-4-08: code that builds a `TenantTx` in a way other than `into_tenant`, and code that runs a query on an `IdentityTx`, do not compile. If `IdentityTx` or `TenantTx` exposes a way to do this, remove it. The pool is a second way: `AppState.auth.pool` is public, so a handler can run a query with no decision. Make the pool private to `auth` and `db`. If that is not possible, add a source check like 7.1 that fails when the code of a route names the pool or the driver. Verify: `cargo test --doc -p strata` passes, and the check fails when a handler reads the pool. Remove the pending marker of TC-4-08.

## 8. Design amendments

- [x] 8.1 Amend 0049, "Data model" and "Roles and grants": `upload_slots` holds `slot_id` and `created_at`, the revision stores `slot_id`, and `strata_api` gets `select (slot_id, created_at)`. Give the reason: a `delete` needs `select` on each column in its `where`. Also amend two more sentences: a finish that sets `FileTooLarge` also removes the row of `upload_slots`, and `slot_id` is a version 4 UUID, an exception to "All identifiers are UUIDs that the database generates with `uuidv7()`". Add rows to "Values that the accepted designs left to implementation" for the limits of the start body and for `Retry-After: 60`. Replace `object_store` with `rusty-s3` at each place that names it, and give the reason of the proposal. Add the note `Amended on <date> (#49)`. Verify: `make lint prose` passes.
- [x] 8.2 Amend 0006, "Upload", step 3: a finish with no object answers `409 Conflict` and changes nothing. Amend each place that names `object_store`: the section "One interface", the reason of O7, the reason about Azure, the new dependency and the Garage check. `rusty-s3` signs the requests, and Azure Blob Storage needs its own client. Add the note `Amended on <date> (#49)`. Verify: `make lint prose` passes.
- [x] 8.3 Amend ADR 0005 (object storage) and ADR 0011 (walking skeleton) with the `domain-modeling` skill: `rusty-s3` replaces `object_store`, with the reason of the proposal. A change to Azure Blob Storage is no longer a change of configuration. Add the note `Amended on <date> (#49)` at each changed place. Verify: `make lint prose` passes.

## 9. Integration and gates

- [x] 9.1 Run `make` and `make l1`. Verify: each target passes. The coverage of `CONSTRAINTS.md` is 85 % or more. `python3 scripts/check-test-cases.py --report` shows each scenario of this change as `implemented`.
- [x] 9.2 Update the implementation-point lists in the comments of #49. Tick each point that this change implements, and name the change that implements each other point. Verify: each point of #5 and #6 that change 2 owns is ticked.

## Workflow follow-up

- This branch starts from the branch of #117. Rebase it on `main` after the owner merges #117, and say so in the pull request.
- Open the pull request with the `pr` skill, and add it to project 1.
- Archive the change with `/openspec-archive-change` after the owner merges the pull request.
