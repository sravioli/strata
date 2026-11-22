Issue: #49

# Proposal

## Why

Tech design 0049 builds the walking skeleton in four changes. This is change 2. It adds the upload of a document. An account starts an upload, sends the bytes to the storage with a signed URL, and finishes the upload. The finish creates the job that the worker of change 3 runs. Change 3 and change 4 need these tables, these routes and the proved revision state function.

## What Changes

- Add a second migration with `documents`, `revisions`, `upload_slots` and `jobs`, with their policies, their grants and the three indexes of `jobs` (0005).
- Add the upload routes of 0049:
  - `POST /v1/documents` starts the upload of a new document.
  - `POST /v1/documents/{document_id}/revisions` starts the upload of the next revision.
  - `POST /v1/documents/{document_id}/revisions/{revision}/finish` finishes an upload.
- Add the storage client with `object_store` and the S3 API (0006). The API signs a PUT URL for one key, with an expiry of 15 minutes. At the finish, the API reads the size of the object.
- Check two facts about Garage before the first L1 test that uses Garage (0006). Garage must accept the signed URLs of `object_store`. Garage must check the expiry only when a request arrives.
- Add the two bounds of the start of an upload: 20,000 `Waiting` jobs (0005) and 5,000 rows in `upload_slots` (0006). At a bound, the API answers `503` with `Retry-After: 60` and logs the rejection.
- Add a task in `strata api` that removes the rows of `upload_slots` that are older than 2 hours, each 60 seconds, 1,000 rows in each batch.
- Add the cleanup function for `Done` jobs that are older than 24 hours, 1,000 rows in each batch. It runs as `strata_worker`. Change 3 calls it from the loop of the worker.
- Add `revision_step` and `may_extract` in `crates/core/src/revision.rs`. Translate them to Lean, and add the five theorems of 0049 and the two refinement theorems in the same pull request.
- Amend 0049 for the key of `upload_slots`, and 0006 for the answer to a finish with no object.

Not in this change:

- `strata worker`, `ClaimTx`, the claim, the lease, the extraction and the transaction that ends a job. Change 3 adds them.
- `strata admin job requeue`. A requeue needs `Parked` jobs, and only the worker of change 3 makes them. Change 3 adds the command. This change adds the grants of `strata_operator` on `jobs`.
- The process that calls the cleanup of `Done` jobs. Only the worker role can remove rows of `jobs`. Change 3 adds that call. Until change 3, no job becomes `Done`.
- `text_parts`, the status routes of 0047, the search and `make dev`. Change 4 adds them. The `Location` of a finish names the status route of 0047 before that route exists.
- The download route of 0006. The skeleton has none ([#118](https://github.com/sravioli/strata/issues/118)).
- The cleanup of revisions in `AwaitingUpload` and of their objects ([#69](https://github.com/sravioli/strata/issues/69)).

## Decisions for this change

The owner decided these points on 2026-11-22:

- **Key of `upload_slots`.** With the grants of 0049, the finish cannot remove its row of `upload_slots`. A test on PostgreSQL 19 gave "permission denied", because a `delete` needs `select` on each column in its `where`. Thus `upload_slots` gets a random key `slot_id`, a version 4 UUID, and the revision stores it. `strata_api` gets `select (slot_id, created_at)` and removes a row by its `slot_id`. The API still cannot read the tenant or the document of a row. This change amends 0049 with this decision.
- **A finish with no object** gets `409 Conflict`, and changes nothing (TC-6-04). This change amends 0006 with this decision.
- **The start body.** `file_name` and `content_type` each have 1 to 1,024 characters. `size` is an integer from 0 to 100 MB. A size above 100 MB gets `413`. Each other value gets `400`. A size of 0 is valid.
- **`Retry-After` at an upload bound** is 60 seconds.

The agent decided these points. The owner reviews them in the pull request:

- 100 MB is 104,857,600 bytes, and 16 KB is 16,384 bytes.
- The start body follows the body rules of 0047: no `Content-Type` gives `400`, a media type other than `application/json` gives `415`, and a body above 16 KB gives `413`. Thus each route with a JSON body has one rule.
- The three routes get their tenant transaction from `authorized_scope(principal, UploadDocument)`. A route with a `document_id` cannot know the tenant of the document before the query. The scope and the policy of 0003 then hide each document of a different tenant, so the route answers `404`.
- A finish that sets `Failed` with `FileTooLarge` also removes the row of `upload_slots`, because the revision is no longer in `AwaitingUpload`. A second finish of that revision answers `413` again. A finish of a revision in each other state, except `AwaitingUpload`, answers `202` with the same `Location`.
- The storage settings come from six environment variables, with the names that `object_store` uses: `AWS_ENDPOINT`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_DEFAULT_REGION`, `AWS_BUCKET_NAME` and `AWS_ALLOW_HTTP`. With no access key or no secret key, `strata api` stops with an error that names the variable. Strata reads only these six and gives each one to the builder, so the client never asks the metadata service of a cloud for credentials.
- `upload_slots` holds only `slot_id` and `created_at`. No query reads its other identifiers, and the API role cannot read them. Thus the table keeps no identifier of a tenant or a document.
- The target of a job is two columns, `document_id` and `revision`. `jobs` has no foreign key, because it is a global table and `revisions` is tenant-owned.

## Test cases

This change carries the test cases that it can make true, each as a scenario with its TC ID:

- 0003: TC-3-04, TC-3-18.
- 0004: TC-4-08.
- 0005: TC-5-01, TC-5-06, TC-5-08, TC-5-10.
- 0006: TC-6-01, TC-6-02, TC-6-03, TC-6-04, TC-6-05.
- 0047: TC-47-09.
- 0049: TC-49-04, TC-49-09.

These test cases stay `pending` in their designs, and a later change or issue carries them:

| Test cases | Reason | Change or issue |
|---|---|---|
| TC-5-02, TC-5-03, TC-5-04, TC-5-05, TC-5-07, TC-5-09, TC-47-16, TC-49-03 | They need the worker. | 3 |
| TC-5-11 | It needs a source of messages that wakes a worker. | [#68](https://github.com/sravioli/strata/issues/68) |
| TC-3-12, TC-4-01, TC-4-03, TC-4-09, TC-49-02 | They need the status routes or the search. | 4 |
| TC-4-02, TC-6-07, TC-6-08 | The skeleton has no removal of a document and no cleanup of `AwaitingUpload`. | [#69](https://github.com/sravioli/strata/issues/69) |
| TC-6-06 | The skeleton has no download route. | [#118](https://github.com/sravioli/strata/issues/118) |
| TC-6-09 | It needs a deployed Strata. | M3 ([#61](https://github.com/sravioli/strata/issues/61)) |

## Capabilities

### New Capabilities

- `document-upload`: the three upload routes, the start body, the bounds, the signed URL, the storage client, the answers of a finish in each state, and the cleanup of `upload_slots`.
- `revision-lifecycle`: the states of a revision, `revision_step`, `may_extract` and their theorems.
- `job-queue`: the table `jobs`, the job that a finish creates, the read of the API on `jobs`, and the cleanup of `Done` jobs.

### Modified Capabilities

- `tenant-isolation`: the new tables follow the three rules (TC-3-04). The list of global tables gets `jobs` and `upload_slots`. The list of grants gets the grants of the new tables. A route opens a tenant transaction only from a decision or a scope (TC-4-08). Each route has a test with the identifiers of a different tenant (TC-3-18).

## Impact

- `crates/strata/migrations/`: a new migration `V2`.
- `crates/strata/src/`: the upload routes, the storage client, the cleanup task and the router of `strata api`.
- `crates/core/src/revision.rs`: new. `lean/Strata/Generated/`, `lean/Strata/Model/Revision.lean` and `lean/Strata/Proofs/Revision.lean` change with it.
- `crates/strata-tests/`: L1 tests against PostgreSQL and Garage. The list of global tables in the test code gets `jobs` and `upload_slots`.
- New dependency: `object_store` with the feature `aws`. `cargo deny` checks it and the crates that it adds.
- `docs/tech-design/0049-walking-skeleton.md` and `docs/tech-design/0006-object-storage.md`: amended notes.
- No data exists, so no migration of data is necessary.
