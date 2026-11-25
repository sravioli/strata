- Issue: [#49](https://github.com/sravioli/strata/issues/49)
- Status: accepted
- Start date: 2026-11-21
- Decided by: sravioli, 2026-11-21

# Walking skeleton: one PDF goes from upload to a tenant-scoped lexical search result

## Summary

The walking skeleton is the shortest path through each component of Strata. An account uploads one PDF. A worker extracts its text in the sandbox of 0007 and writes it to a lexical index in PostgreSQL. The client polls the status of the revision until it is `ready`. Then a search of the same tenant finds the document, and a search of a different tenant does not. This design defines the lifecycle of a revision, the search index and the search route. It also gives the values that the accepted designs left to implementation. It does not change a decision of those designs, except at the places with an "Amended on" note.

## Motivation

The accepted designs decide each part alone. They are tenancy (0003), authorization (0004), the job queue (0005) and object storage (0006). Also the extraction sandbox (0007), identity (0021) and readiness (0047). No design connects them. A skeleton that runs end to end shows the faults between the parts early, when they are cheap to fix. Later work then adds width to a path that works: more formats ([#102](https://github.com/sravioli/strata/issues/102)), PDFium and the other limits ([#103](https://github.com/sravioli/strata/issues/103)), and hybrid search (M2).

Three questions belong to this design:

- The lifecycle of a revision and its internal state names. The map of 0047 uses those names.
- The search index and the search route. No design decided them.
- The implementation points that the accepted designs gave to this issue. The two lists in the comments of [#49](https://github.com/sravioli/strata/issues/49) hold them.

## Guide-level explanation

### One upload, from start to search

Alice works for the tenant `acme`. Her product backend uploads a PDF and searches for it.

1. The backend gets a token from the issuer of `acme` and calls `POST /v1/documents` with the file name, the content type and the size. The API creates the document, revision 1 in the state `AwaitingUpload`, and a signed PUT URL. The public status is `uploading`.
2. The backend sends the bytes to the storage with the URL.
3. The backend calls `POST /v1/documents/{document_id}/revisions/1/finish`. The API checks the size, sets the revision to `Uploaded` and creates its job, in one transaction. It answers `202` with the URL of the status. The public status is `processing`.
4. A worker claims the job and starts `strata-extract` in nsjail. The extraction process reads the PDF with `pdftotext` and writes one block of text for each page.
5. The worker writes the text parts, sets the revision to `Ready`, and ends the job, in one transaction. The public status is `ready`.
6. The backend polls `GET /v1/documents/{document_id}/revisions/1` until the status is final.
7. The backend calls `QUERY /v1/search` with `{"q": "quarterly revenue"}`. The answer holds the document, its revision, a score and the page of the best match.

A backend of the tenant `globex` sends the same search. The answer is empty, because the search reads only rows of the tenant of the caller.

### A new revision of the same document

Alice uploads a new file as revision 2 of the same document, with `POST /v1/documents/{document_id}/revisions`. Revision 1 stays searchable until revision 2 is `Ready`. In the transaction that sets revision 2 to `Ready`, revision 1 becomes `Superseded`, and its search rows go. A search then finds revision 2 only. A client cannot go back to revision 1 in v1 ([#101](https://github.com/sravioli/strata/issues/101)).

### Locally

A developer runs `make dev`. It starts PostgreSQL, the pooler and Garage. It applies the migrations, starts `strata-dev`, creates two tenants and binds the development issuer to them. Then it runs `strata api` and `strata worker`. `strata-dev token --tenant acme --sub alice` gives a token for the first call. Development needs Linux, because nsjail needs Linux user namespaces.

## Reference-level explanation

### Crates and processes

| Crate | Kind | Contents |
|---|---|---|
| `core` | library | The proved pure functions: authorization, the job state, and the revision state (new). No I/O. |
| `strata` | binary | The server. Subcommands `api`, `worker`, `admin` and `migrate`. All database and storage code. |
| `strata-extract` | binary | The extraction process of 0007. It has no code for a database, a storage or a network. |
| `strata-dev` | binary | The development issuer of 0021. The `strata` crate does not depend on it (TC-21-17). |

Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): a fifth crate, `strata-tests`, holds the L1 tests. It depends on `strata` and on `strata-dev`, so the tests can sign tokens, and `strata` keeps no dependency on `strata-dev` of any kind (TC-21-17).

`strata api` and `strata worker` run as separate processes and scale separately (ADR 0003). `strata admin` is the operator command of 0003 and 0021. It connects with the role `strata_operator`.

Libraries:

- axum 0.8 and tokio for HTTP,
- tokio-postgres 0.7 with deadpool-postgres 0.14 for PostgreSQL, and refinery 0.10 for the migrations,
- `rusty-s3` 0.12 for the signatures of the storage requests (0006). Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): it was `object_store` 0.14. With the feature `aws`, `object_store` adds `aws-lc-rs`, a second crypto backend. With each set of features, `make deny` fails, because `object_store` depends on `chrono` and `humantime`, and each has an advisory that it is not maintained (RUSTSEC-2026-0542, RUSTSEC-2026-0543). `rusty-s3` only signs requests, and the HTTP client of `strata` sends the requests of the API (owner, 2026-11-22),
- `jsonwebtoken` for tokens, and `clap` for the subcommands.

Queries are checked at run time and by L1 tests, so the build needs no database.

Amended on 2026-11-22 ([#100](https://github.com/sravioli/strata/issues/100)): the prototype [#100](https://github.com/sravioli/strata/issues/100) measured sqlx against tokio-postgres. The rule of the owner was: keep sqlx, unless tokio-postgres is better by 20 % or more for one query shape or more (2026-11-21). The rule applies to p99 latency and to throughput. tokio-postgres was better on the tenant transaction (+115 % to +372 % throughput) and on the batch status read (+61 % to +316 %). This was true on each path, through PgBouncer too. The search and the batch insert did not change. Thus tokio-postgres replaces sqlx, and refinery replaces `sqlx migrate`.

**Batching rule.** Each step reads or writes its rows with one statement. Examples: the text parts of a revision go in with one `INSERT … SELECT … FROM unnest(…)`, and the batch status read is one `SELECT`. A loop of statements for single rows is not permitted.

**Pipelining rule.** When a statement does not need the result of an earlier statement in the same transaction, the code sends the statements together on one connection. Example: the tenant transaction of a read sends `BEGIN`, `set_config`, the query and `COMMIT` together. This removes three round trips. If one statement fails, PostgreSQL aborts the transaction, the next statements fail, and `COMMIT` gives a rollback. The code then returns the first error. A statement that needs an earlier result waits for it. Example: the transaction that ends a job locks the document before it writes.

### Identifiers

All identifiers are UUIDs that the database generates with `uuidv7()`. Version 7 UUIDs start with the time, so new rows go to the end of each B-tree index. PostgreSQL 18 added `uuidv7()`, and PostgreSQL 17 does not have it. The skeleton moves the pins of `compose.yaml` and of the `sql` job of CI to PostgreSQL 19, the latest version (owner, 2026-11-21). One change moves both pins. PostgreSQL 19 was released on 2026-11-19. If an image, PgBouncer or sqlx does not work with it yet, the first change uses PostgreSQL 18 and records the reason.

Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): one identifier is an exception. `slot_id` of `upload_slots` is a version 4 UUID from `gen_random_uuid()`. It is a random key, and no index of a tenant table holds it.

The proved functions of `core` use `u64` for the identifiers of a tenant and an account. A UUID has 128 bits. The first change of the skeleton changes these fields to `u128`, and the `strata` crate gives the 128 bits of the UUID. The proofs compare identifiers for equality only, so they do not change. The translation and the proofs change in the same pull request (`docs/agents/testing.md`).

Tenant names: 1 to 63 characters, lowercase letters `a` to `z`, digits and `-`. The first character is a letter. The operator command rejects another name.

### Data model

These tables are new. Each table that a tenant owns follows the three rules of 0003: `tenant_id` is not null, the primary key starts with `tenant_id`, and each foreign key includes `tenant_id`. Each one has the one policy of 0003, row-level security enabled and forced.

`documents`:

| Column | Meaning |
|---|---|
| `tenant_id`, `document_id` | The primary key |
| `last_revision` | The number of the last revision that a start of an upload created |
| `ready_revision` | The number of the `Ready` revision, or null. The worker writes it in the transaction that ends a job. |
| `created_at` | The time of the first upload |

`revisions`:

| Column | Meaning |
|---|---|
| `tenant_id`, `document_id`, `revision` | The primary key |
| `state` | `AwaitingUpload`, `Uploaded`, `Ready`, `Failed` or `Superseded` |
| `file_name`, `content_type`, `size` | What the client sent at the start (0006) |
| `retry_mark` | True after the first crash, time limit or invalid output (0007) |
| `error_class` | The class of a `Failed` revision (0007, 0006). Null in each other state. |
| `no_text` | True when a `Ready` revision has no text (`NoText` of 0007) |
| `status_at` | The time of the last change of the public status (`updated_at` of 0047) |
| `slot_id` | The key of the row of `upload_slots` of the start. Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)). |
| `created_at` | The time of the start |

`text_parts`:

| Column | Meaning |
|---|---|
| `tenant_id`, `document_id`, `revision`, `part` | The primary key. `part` counts from 1 in the order of the text. |
| `location_kind`, `location_number` | `page`, `sheet` or `slide`, and its number from 1 |
| `part_text` | At most 64 KB of text |
| `tsv` | `tsvector`, generated and stored: `to_tsvector('simple', part_text)` |

Index: `create index on text_parts using gin (tenant_id, tsv)`, with the extension `btree_gin`, which has an operator class for `uuid`. Thus a search reads only the index entries of one tenant, and its cost does not depend on the count of tenants (S5 of 0002). Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): under the policy of 0003, PostgreSQL uses the index for `tsv @@ query` only when the function `ts_match_vq` is `leakproof`. Thus the setup of the database marks `ts_match_vq(tsvector, tsquery)` as `leakproof`, as a superuser (owner, 2026-11-22). The columns `location` and `text` became `location_number` and `part_text`, because `sqlfluff` rejects SQL keywords as column names (agent decision, for review in [#122](https://github.com/sravioli/strata/pull/122)). Text parts exist only for a revision in the state `Ready`. The worker writes them in the transaction that sets `Ready`. The transaction that sets `Superseded` removes them.

`upload_slots` is a global table, as `jobs` is (0005). It holds no tenant content, only a random key and a time:

| Column | Meaning |
|---|---|
| `slot_id` | The primary key, a version 4 UUID. The revision stores it. |
| `created_at` | The time of the start |

Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): the table held `tenant_id`, `document_id` and `revision`. With `select (created_at)` only, the finish could not remove its row: a `delete` needs `select` on each column in its `where`. A test on PostgreSQL 19 gave "permission denied". Thus the row has a random key, and the finish removes the row by that key. The API still cannot read the tenant or the document of a row, because the table has no such column (owner, 2026-11-22).

The start of an upload inserts a row, and the finish removes it. A finish that sets `Failed` with `FileTooLarge` also removes it, because the revision is no longer in `AwaitingUpload` (Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49))). The bound on revisions in `AwaitingUpload` of 0006 counts its rows from the last 2 hours: `select count(*) from upload_slots where created_at > now() - interval '2 hours'`. A count of `revisions` cannot give that number, because the policy of 0003 shows a role only the rows of one tenant. A task removes rows older than 2 hours. It needs no tenant context, because the table is global.

**Revision numbers.** The start of an upload with a `document_id` runs `update documents set last_revision = last_revision + 1 where tenant_id = $1 and document_id = $2 returning last_revision`. The row lock of that update puts two starts for one document in sequence, so each gets its own number. A start with no `document_id` inserts the document with `last_revision = 1`.

**Roles and grants.** The roles are the four roles of 0003. `strata_api` gets `select`, `insert` and `update` on `documents` and `revisions`, and `select` on `text_parts`. It gets `insert` and `select (state)` on `jobs` (0005, TC-5-08). It gets `insert`, `delete` and `select (slot_id, created_at)` on `upload_slots`. Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): it was `select (created_at)`, see `upload_slots` above. `strata_worker` gets `select` and `update` on `revisions`, and `select`, `insert` and `delete` on `text_parts`. It gets `select` and `update (ready_revision)` on `documents`, because a row lock with `for update` needs the `update` privilege on one column or more. It gets `select`, `update` and `delete` on `jobs`. Implementation writes these grants in the migrations, and an L1 test checks each grant that this list does not give.

### The lifecycle of a revision

| State | Public status (0047) | It ends when |
|---|---|---|
| `AwaitingUpload` | `uploading` | The finish of the upload, or the cleanup after 2 hours (0006) |
| `Uploaded` | `processing` | The end of the extraction |
| `Ready` | `ready` | A newer revision of the document becomes `Ready` |
| `Failed` | `failed` | Never |
| `Superseded` | `superseded` | Never |

The events:

| Event | From | To |
|---|---|---|
| `Finish` | `AwaitingUpload` | `Uploaded` |
| `FinishTooLarge` | `AwaitingUpload` | `Failed`, class `FileTooLarge` |
| `Extracted(no_text)` | `Uploaded` | `Ready`, with `no_text` |
| `ExtractFailed(class)` with a class that retries: `ExtractionCrash`, `ExtractionTimeout`, `InvalidOutput` | `Uploaded` | `Uploaded` with `retry_mark`, when the mark was not set. `Failed` with the class, when the mark was set. |
| `ExtractFailed(class)` with each other class of 0007 | `Uploaded` | `Failed` with the class |
| `NewerReady` | `AwaitingUpload`, `Uploaded`, `Ready` | `Superseded` |

Each pair of a state and an event that the table does not name leaves the revision as it is. Thus a `Failed` or `Superseded` revision never changes, and a `Ready` revision can become only `Superseded`. These are the rules of 0047.

The pure function `revision_step` in `core` holds this table. A second pure function, `may_extract`, is true only for `Uploaded`. The worker and the API call these functions and write their result. They do not decide a transition in SQL. The rules for several revisions of one document, for example "only one revision is `Ready`", are route 2 of 0009 and belong to [#78](https://github.com/sravioli/strata/issues/78). The transaction below makes them true. L1 tests check them.

### The transaction that ends a job

The worker ends a `ProcessDocument` job with one tenant transaction:

1. Lock the row of the document: `select … from documents where tenant_id = $1 and document_id = $2 for update`. A second job of the same document waits here.
2. Read the revision, and apply `revision_step` to the result of the extraction.
3. When the new state is `Ready`:
   - Insert the text parts with one statement.
   - Set `ready_revision` of the document to this revision.
   - Set each older revision of the document in `AwaitingUpload`, `Uploaded` or `Ready` to `Superseded`, and set its `status_at`. One statement does this.
   - Remove the text parts of those revisions with one statement.
4. Write the revision. End the job with a check of `lease_token` (0005). A retry ends the job with `Fail`, and each other result ends it with `Succeed`.

Step 1 also covers the case where an older job ends after a newer revision is `Ready`. The older revision is then `Superseded`, `revision_step` leaves it as it is, and the job ends with `Succeed` and no write of text.

At the claim, the worker reads the revision. When `may_extract` is false, the job ends with `Succeed`, and the worker starts no extraction. Thus the job of a `Superseded` revision does no extraction (0047).

### Identity, decisions and transactions in Rust

The types in the `strata` crate make the rules of 0004 hold at compile time:

- `IdentityTx` wraps a transaction on the primary. Its only methods read the issuer binding, read or create the account, and read the two statuses (0021, steps 7 to 10).
- `Allowed` is a value that only `allowed(decision: Decision) -> Option<Allowed>` makes, and only from `Decision::Allow`. A `Scope` comes only from `authorized_scope` (0004).
- `IdentityTx::into_tenant(self, Allowed or Scope) -> TenantTx` is the one way to open a tenant transaction. It sets `app.tenant_id` with `set_config(…, true)` (0003).
- Each query function takes `&mut TenantTx`. No query function takes a pool or a connection. Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): a query function that ends the transaction with its statement takes `TenantTx` by value. It sends `commit` with the statement (the pipelining rule). The rule applies to the tenant-owned tables. The cleanup of the global tables `jobs` and `upload_slots` takes a connection of its role, because it needs no tenant context.
- The worker has `ClaimTx`. It reads the job, sets the tenant context of the job and reads the tenant status. Then `job_step` and `authorize` decide, and `ClaimTx::into_tenant` gives the `TenantTx` of the job (0004).

The test helper `tenant_tx(tenant)` opens a tenant transaction for tests through the same function. Tests do not open one another way (0003).

### Upload routes

| Route | Action (0004) | Result |
|---|---|---|
| `POST /v1/documents` | `UploadDocument` | Start of an upload of a new document. `201`, with `document_id`, `revision` (1), the signed PUT URL and its expiry. |
| `POST /v1/documents/{document_id}/revisions` | `UploadDocument` | Start of an upload of the next revision. `201`, with the same fields. A document of a different tenant gives the same `404` as a document that does not exist (0006). |
| `POST /v1/documents/{document_id}/revisions/{revision}/finish` | `UploadDocument` | `202`, with `Location` (0047) |

Each route of a revision is under the path of its status, `/v1/documents/{document_id}/revisions/{revision}` (0047). The `document_id` of 0006 is thus in the path, not in the body. The collections have plural names, as the routes of 0047 do.

The start rejects a size above 100 MB with `413`, so the client learns early. The finish still checks the real size of the object, because the signed URL cannot limit it (0006).

The finish runs in one tenant transaction. It locks the revision with `select … for update`, and applies `revision_step` with `Finish` or `FinishTooLarge`. It creates the job only when the revision changes from `AwaitingUpload` to `Uploaded`. It removes the row of `upload_slots` when the revision leaves `AwaitingUpload`, to `Uploaded` or to `Failed` with `FileTooLarge` (Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49))). Thus a second finish, or two finishes at the same time, create one job only (0006).

### Extraction in the skeleton

The skeleton has only PDF, with the program `pdftotext` (`accurate`). It does not have the field `pdf_parser` or PDFium ([#103](https://github.com/sravioli/strata/issues/103)). It runs in nsjail with the limits of 0007. They are 1 GB of memory, 60 seconds of CPU, 120 seconds in total and 50 MB of output. A file in a different format gets `UnsupportedFormat`.

The output of `strata-extract` is one JSON document:

```json
{"format": "pdf", "pages": 2, "blocks": [{"kind": "page", "index": 1, "text": "…"}]}
```

The worker checks it as 0007 says. It splits a block above 64 KB at the last whitespace before 64 KB, or at 64 KB when the block has no whitespace there. A `tsvector` must be less than 1 MB, and it records positions only up to 16,383 (PostgreSQL documentation, "Limitations" of text search). A part of 64 KB stays far below both limits.

### Search

`QUERY /v1/search`, with the action `Search` (0004). The request body:

```json
{"q": "quarterly revenue", "limit": 20}
```

- `q` is 1 to 1,000 characters. `limit` is 1 to 100, and 20 when it is absent. Other values get `400`.
- The body rules of the batch route of 0047 apply: no `Content-Type` gives `400`, another media type gives `415`, and a body above 16 KB gives `413`.
- The query is `websearch_to_tsquery('simple', $q)`. That function never raises a syntax error on user input. Quotes give a phrase, `or` gives either word, and `-word` excludes a word.
- The search reads `text_parts` of the scope of the caller only. It ranks each part with `ts_rank`. It keeps the best part of each document, and returns at most `limit` documents, best first. Amended on 2026-11-25 ([#145](https://github.com/sravioli/strata/issues/145)): the search keeps the best part of each location, and returns at most `limit` hits, best first (tech design 0145).

The response:

```json
{"results": [{"document_id": "7b1e…", "revision": 2, "score": 0.42, "location": {"kind": "page", "number": 3}}]}
```

The response has no snippets and no pages of results. The design of the search path ([#73](https://github.com/sravioli/strata/issues/73)) decides them, and also ranking other than `ts_rank`. Amended on 2026-11-25 ([#145](https://github.com/sravioli/strata/issues/145)): each hit has a snippet (tech design 0145). #73 still decides the pages of results and the ranking.

The search must rank each match in the tenant. Thus its cost grows with the count of matches in one tenant, and S5 of 0002 does not hold for it. The search has a statement timeout of 2 seconds, and a timeout gives `503` with `Retry-After` (S4 of 0002). [#73](https://github.com/sravioli/strata/issues/73) must remove this limit before M2 ends.

**The method `QUERY`.** axum 0.8 has no method filter for `QUERY`. One handler at each path of `QUERY` checks the method and answers `405` with `Allow` for another method. The search route and the batch route of 0047 use the same handler type.

### Values that the accepted designs left to implementation

| Value | Design | Decision | Reason |
|---|---|---|---|
| Connections in the pool of each process | 0002 | 20 | Below the connection bound of a small PostgreSQL, with 4 processes on one machine |
| Wait for a connection | 0002 | 2 s, then `503` with `Retry-After: 1` | Below the p99 target of 1 s plus one retry |
| Requests that one API process handles at the same time | 0002 | 256 | 13 requests for each connection, for short queries |
| Size of a request body | 0002 | 16 KB, the limit of 0047 | The API has no large JSON body |
| Lease | 0005 | 300 s, the value in `job.rs` | Longer than two extractions of 120 s |
| Renewal of a lease | 0005 | Each 100 s | Three renewals in each lease |
| Poll of a worker with no job | 0005 | 1 s, two times longer after each empty poll, up to 10 s | Fast work when jobs arrive, few queries when the queue is empty |
| Jobs that one worker runs at the same time, and extraction processes of one worker | 0005, 0007 | 2 | Two sandboxes of 1 GB on a small machine |
| Bound on `Waiting` jobs | 0005 | 20,000 | About 2 hours of uploads at the ceiling of 10,000 each hour (ADR 0003) |
| Start body | 0006, 0047 | `file_name` and `content_type` have 1 to 1,024 characters. `size` is an integer from 0 to 104,857,600 (100 MB). A size above 100 MB gives `413`. Each other wrong value gives `400`. The body rules of 0047 apply: no `Content-Type` gives `400`, another media type gives `415`, and a body above 16,384 bytes gives `413`. Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)). | The owner decided the limits on 2026-11-22. Each route with a JSON body has the same body rules. |
| `Retry-After` of a start at a bound | 0005, 0006 | 60 s. Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)). | The owner decided it on 2026-11-22. |
| Bound on revisions in `AwaitingUpload`, for all tenants | 0006 | 5,000, counted in `upload_slots` | Above the approximately 2,700 uploads that 15 minutes hold at the ceiling. Objects that are not finished then hold approximately 500 GB, when each client sends the size that it declared. The signed URL cannot limit the real size (0006), so a client can put more. |
| Cleanup | 0005, 0006 | Each 60 s, 1,000 rows each batch | Done jobs after 24 hours, and rows of `upload_slots` after 2 hours. The cleanup of revisions in `AwaitingUpload` and of the objects of `Superseded` revisions waits for its principal ([#69](https://github.com/sravioli/strata/issues/69)). |
| Token buckets of one API process | 0047 | 100,000, least recently used goes first | Ten buckets for each of the 10,000 accounts of C1 (ADR 0003) |
| Fixed list of error classes of a job | 0005 | `NoError`, `TenantNotActive`, `LeaseExpired`, `JobFailed`, the list in `job.rs` | The class of an extraction goes to the revision. See the amended note in 0005. |

### Local services and `make dev`

`compose.yaml` gets PgBouncer in transaction mode, with `max_prepared_statements` above 0, so that the prepared statements of tokio-postgres work through it. The prototype [#100](https://github.com/sravioli/strata/issues/100) used 200. `strata api` and `strata worker` connect through PgBouncer. The migrations and `strata admin` connect to PostgreSQL directly. The L1 test of 0003 for prepared statements through the pooler runs against it.

`make dev`:

1. `docker compose up --wait`: PostgreSQL 19, PgBouncer and Garage.
2. `strata migrate` as `strata_owner`. refinery embeds the SQL files of `migrations/` in the binary.
3. Start `strata-dev serve`. It prints the commands that register its issuer (0021).
4. `strata admin tenant create acme`, the same for `globex`, the registration of the issuer, and one binding for each tenant.
5. Start `strata api` and `strata worker` on the host.

Before the first L1 test with Garage, the second change checks two things. The first check: Garage accepts the signed URLs of `rusty-s3` (Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): it was `object_store`). The second check: Garage checks the expiry only when the request arrives (0006).

### Operator commands

```text
strata admin tenant create <name>
strata admin tenant set-status <name> active|not-active
strata admin tenant list
strata admin issuer register --issuer <url> --jwks-url <url> --audience <aud> --algorithm RS256|ES256|EdDSA --tenant-claim <claim> [--account-claim <claim>] [--max-lifetime <s>]
strata admin issuer set <issuer> [the options of register] [--status active|not-active]
strata admin binding add --issuer <issuer> --tenant-value <value> --tenant <name>
strata admin binding remove --issuer <issuer> --tenant-value <value>
strata admin account set-status --tenant <name> --issuer <issuer> --subject <sub> active|not-active
strata admin job requeue --tenant <name>
```

These are the tasks of 0003, 0005 and 0021.

### How the work lands

One OpenSpec change for each step, each in its own pull request, in this order:

1. Schema, tenancy and identity. The tables of 0003 and 0021, the roles, `uuidv7()` and PostgreSQL 19, `u128` in `core`, the types of the transactions, `strata admin`, `strata-dev`, and the authentication of the API. The prototype [#100](https://github.com/sravioli/strata/issues/100) chose tokio-postgres.
2. Upload. `documents`, `revisions`, `upload_slots`, `jobs`, the upload routes, the Garage checks, the cleanup of `jobs` and `upload_slots`, and `revision_step` with its proofs.
3. The worker and extraction. The claim, the lease, `strata-extract` with `pdftotext` in nsjail, `text_parts` with its index and its grants, the transaction that ends a job, and `strata admin job requeue`.
4. Search and status. The search route, the status routes of 0047, `make dev`, and TC-49-01.

Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): `text_parts` moves from the fourth change to the third change. The transaction that ends a job writes the text parts when a revision becomes `Ready`. Without the table, the third change sets `Ready` with no text. `strata admin job requeue` is also in the third change, because it belongs to the job lifecycle of 0005 (owner, 2026-11-22).

## Proof obligations

| Theorem | Requirement it covers | Plain-English statement | Lean statement | Rust function it is about |
|---|---|---|---|---|
| `final_sticky` | 0047: a final status does not change, except from `ready` to `superseded` | A revision that is `Failed` or `Superseded` stays in that state for each event. | `(r.state = .Failed ∨ r.state = .Superseded) → revision_step r e = ok r` | `revision_step` |
| `ready_only_superseded` | 0047: `ready` can become only `superseded` | A `Ready` revision stays `Ready` or becomes `Superseded`. It never becomes `Failed`. | `r.state = .Ready → ∃ r', revision_step r e = ok r' ∧ (r'.state = .Ready ∨ r'.state = .Superseded)` | `revision_step` |
| `retry_once` | 0007: a crash, the time limit or invalid output retries one time | A crash, the time limit or invalid output on an `Uploaded` revision with no retry mark sets the mark, and the revision stays `Uploaded`. With the mark, the same result sets `Failed` with that class. | `r.state = .Uploaded → retries c → ∃ r', revision_step r (.ExtractFailed c) = ok r' ∧ (r.retry_mark = false → r'.state = .Uploaded ∧ r'.retry_mark = true) ∧ (r.retry_mark = true → r'.state = .Failed ∧ r'.error_class = c)` | `revision_step` |
| `extract_only_uploaded` | 0047: the job of a `superseded` revision does no extraction | The worker may extract a revision only when it is `Uploaded`. | `may_extract r = ok b → (b = true ↔ r.state = .Uploaded)` | `may_extract` |
| `revision_total` | 0009: the proved functions never stop with an error | The two functions give a result for each input and never stop with an error, and the result is the result of the model. | `∀ r e, ∃ r', revision_step r e = ok r' ∧ …` and the same for `may_extract` | `revision_step`, `may_extract` |

`Strata.revision_step_refines` and `Strata.may_extract_refines` show that each function agrees with the model in `Strata/Model/Revision.lean` (0009). The proofs rest on one assumption: the transaction that ends a job is atomic. Thus the state that the worker writes is the result of one call of `revision_step`.

## Impacts

### Infrastructure

- New processes: `strata api`, `strata worker`, `strata-extract` (one for each file) and `strata-dev` (local only).
- New local service: PgBouncer. PostgreSQL moves from 17 to 18.
- Logs: one line for each request and each job, with no tenant content (0005). Metrics for each bound of 0002 S6 come with M3 ([#61](https://github.com/sravioli/strata/issues/61)). The skeleton logs each rejection at a bound.
- TC-49-01 runs nsjail in CI, so the CI runner must permit user namespaces (0007, 0048).
- The skeleton runs only locally and in CI. It has no deployment and no release. The licence text of Poppler in the image waits for the first release ([#105](https://github.com/sravioli/strata/issues/105)).

### Security and tenant isolation

- Each query function takes a `TenantTx`, and only a decision or a scope opens one (0004). The search reads `text_parts` with the scope of the caller, and the policy of 0003 also filters it.
- The GIN index starts with `tenant_id`, so a search of one tenant never reads the index entries of another tenant.
- The search text goes in the body of `QUERY`, so it is not in URLs or in the access logs of a proxy.
- `strata-dev` is not in the `strata` crate (TC-21-17). Its key and its tokens exist only locally.
- An exploit of `pdftotext` reaches one file, as 0007 says.

### Data and migration

- New tables: the tables of 0003 and 0021, `documents`, `revisions`, `text_parts`, `jobs`. New extension: `btree_gin`.
- No data exists, so no migration of data is necessary.
- `core` changes the type of two identifiers from `u64` to `u128`. Its Lean translation changes in the same pull request.

## Test cases

L0: unit tests cover the split of blocks, the check of the request body and the map from internal state to public status. They also cover the parse of the output of `strata-extract`. The theorems above cover the lifecycle of one revision.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-49-01 | L2 | Given the walking skeleton locally, with the tenants A and B and an account of each. When the account of A uploads a PDF with a unique word, and polls the status of the revision until it is final. Then the status is `ready`. A search of A for the word returns the document. The same search of B returns no result. CI runs this test on each pull request that changes Rust code, and before each release. | sravioli, 2026-11-21 | pending |
| TC-49-02 | L1 | Given revision 1 of a document is `Ready`. When revision 2 becomes `Ready`. Then revision 1 is `Superseded`, its text parts do not exist, and a search returns revision 2 only. | sravioli, 2026-11-21 | pending |
| TC-49-03 | L1 | Given revision 2 of a document is `Ready`. When the job of revision 1 ends after that. Then revision 1 stays `Superseded`, the worker writes no text for it, and the job is `Done`. | sravioli, 2026-11-21 | pending |
| TC-49-04 | L1 | Given two starts of an upload for the same `document_id` at the same time. When both complete. Then they get two different revision numbers, one after the other. | sravioli, 2026-11-21 | pending |
| TC-49-05 | L1 | Given documents of tenants A and B with the same word. When A searches for the word. Then each result is a document of A. | sravioli, 2026-11-21 | pending |
| TC-49-06 | L1 | Given a page of 200 KB of text. When the worker writes it. Then it is in four parts of at most 64 KB, and a search for a word in the last part returns the page. | sravioli, 2026-11-21 | pending |
| TC-49-07 | L1 | Given a search body with no `q`, with a `limit` of 101, or above 16 KB. When the client sends it. Then the API answers `400`, `400` and `413`. | sravioli, 2026-11-21 | pending |
| TC-49-08 | L1 | Given a `GET` and a `POST` to `/v1/search`. When the API gets them. Then it answers `405` with the header `Allow: QUERY`. | sravioli, 2026-11-21 | pending |
| TC-49-09 | L1 | Given a revision in `AwaitingUpload` with its object in the storage. When the client sends two finishes at the same time, and then a third. Then the revision is `Uploaded`, it has exactly one job, and each finish answers `202` with the same `Location`. | sravioli, 2026-11-21 | pending |
| TC-49-10 | L1 | Given the nsjail configuration of `strata-extract`. When the extraction process calls `io_uring_setup`, `io_uring_enter` or `io_uring_register`. Then each call fails, and the process does no I/O through a ring. Reason: seccomp does not see the operations of a ring, so a ring can bypass the seccomp list of 0007. | sravioli, 2026-11-22 | pending |

TC-49-01 is TC-47-11 with the search of a second tenant. The owner decided it on 2026-11-21. It runs in CI also, not only before a release. That is a change to `docs/agents/testing.md` for this one test, and the fourth change makes it.

Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): the third change has TC-49-03. It also checks the clauses of TC-49-02 and TC-49-06 on text parts, with a `select` on `text_parts`. The clauses of TC-49-02 and TC-49-06 on the search come with the search route in the fourth change (owner, 2026-11-22).

## Drawbacks

- The skeleton extracts only PDF. A client cannot use it for other formats until [#102](https://github.com/sravioli/strata/issues/102).
- The cost of a search grows with the matches in one tenant, until [#73](https://github.com/sravioli/strata/issues/73).
- Development needs Linux.
- PostgreSQL 19 is new, and a managed service can offer it months later than 17 (M3, [#61](https://github.com/sravioli/strata/issues/61)).

## Rationale and alternatives

**A skeleton with all formats and both PDF parsers.** It gives a complete extraction at once. But it makes the first path much longer, and a fault between two parts shows later. The owner selected one format in the real sandbox.

**Extraction in the worker process first, and the sandbox later.** It is faster to build. But it breaks the main rule of 0007, and a later change of the security boundary is risky. The owner rejected it.

**A separate search engine, for example ParadeDB or Tantivy.** BM25 ranks better than `ts_rank`. But it adds a service or an extension, and the tenant policy of 0003 must then also cover it. [#73](https://github.com/sravioli/strata/issues/73) decides the search path of M2.

**One search row for each revision.** It is simpler. But a `tsvector` must be less than 1 MB, and a revision can have 50 MB of text. A row for each page also gives the location of a match.

**States for "extracted" and "indexed".** They show more progress. But the worker writes the text and the index in one transaction, so no reader can see the state between them. Five states map one to one to the five public statuses.

**Keep the search rows of a `Superseded` revision.** A later restore then needs no extraction. But it costs storage now, and the cleanup of 0006 already removes the file. The owner moved restore to [#101](https://github.com/sravioli/strata/issues/101).

**sqlx instead of tokio-postgres and deadpool.** sqlx gives the pool, the migrations and the map of rows in one crate. But it sends one statement at a time, and the prototype [#100](https://github.com/sravioli/strata/issues/100) measured tokio-postgres better by more than 20 % on two query shapes. Amended on 2026-11-22 ([#100](https://github.com/sravioli/strata/issues/100)).

**`sqlx-cli` for the migrations only.** The SQL files then stay as they are. But each environment then needs one more binary, and its version must agree with the files. refinery puts the migrations in the `strata` binary.

**io_uring for the I/O of the driver.** The gain of [#100](https://github.com/sravioli/strata/issues/100) comes from fewer round trips, not from the cost of a system call. A runtime with io_uring also replaces tokio for axum and the other libraries, and the default seccomp profile of Docker blocks io_uring. Rejected. PostgreSQL itself can use io_uring for disk reads ([#113](https://github.com/sravioli/strata/issues/113)).

**`GET /v1/search?q=…`.** `GET` works with each client. But the search text then goes in the URL and in access logs, and 0047 already uses `QUERY` for a read with a body.

**UUIDs from the application.** PostgreSQL 17 then needs no upgrade. But 0003 decides that the database generates the identifiers, and `uuidv7()` does that with no extension from PostgreSQL 18.

## Prior art

- The PostgreSQL documentation of `btree_gin` recommends one multicolumn GIN index over two indexes that a bitmap AND must combine. The index on `(tenant_id, tsv)` follows it.
- Alistair Cockburn named the walking skeleton: the thinnest slice of real function that runs end to end. Freeman and Pryce use it in "Growing Object-Oriented Software, Guided by Tests".

## Unresolved questions

The owner answered these questions on 2026-11-21:

1. The skeleton has the real sandbox, with PDF and `pdftotext` only.
2. The search uses PostgreSQL full-text search with the configuration `simple`.
3. The revision has five internal states, and the other facts are columns.
4. Four crates, and one server binary with subcommands.
5. axum, tokio, sqlx, `object_store` and `jsonwebtoken`. Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): `rusty-s3` replaces `object_store`, see "Crates and processes". The prototype [#100](https://github.com/sravioli/strata/issues/100) checks sqlx with a threshold of 20 %. Amended on 2026-11-22 ([#100](https://github.com/sravioli/strata/issues/100)): tokio-postgres and deadpool-postgres replace sqlx, and refinery replaces `sqlx migrate`.
6. `make dev`, and development on Linux only.
7. The revision state function is proved, with the statements of the Proof obligations table.
8. The agent sets the values of the implementation points, and the owner reviews them in this pull request.
9. One row for each page, sheet or slide, and a split at 64 KB.
10. `QUERY /v1/search`, with one result for each document and no snippets. Amended on 2026-11-25 ([#145](https://github.com/sravioli/strata/issues/145)): one hit for each matching location, with a snippet (tech design 0145).
11. The search rows of a `Superseded` revision go. Restore is [#101](https://github.com/sravioli/strata/issues/101).
12. The test case TC-49-01, which also runs in CI.
13. Each step reads or writes its rows with one statement.
14. The skeleton uses PostgreSQL 19, the latest version, and `core` uses `u128` for identifiers.
15. A global table `upload_slots` counts the revisions in `AwaitingUpload` for the bound of 0006.
16. PgBouncer in transaction mode is in `compose.yaml`, and `strata api` and `strata worker` connect through it.
17. The upload routes are `POST /v1/documents`, `POST /v1/documents/{document_id}/revisions` and `POST /v1/documents/{document_id}/revisions/{revision}/finish`.
18. The owner approved the values in "Values that the accepted designs left to implementation", and the limit of 1 to 1,000 characters on `q`.
19. The owner approved the amended notes in 0005, 0006, 0047, ADR 0009 and 0048.
20. The skeleton does not clean up revisions in `AwaitingUpload` or the objects of `Superseded` revisions. That cleanup waits for its principal in [#69](https://github.com/sravioli/strata/issues/69). Until then, such revisions and their objects stay. The issue #69 records this.
21. The owner approved the test cases TC-49-02 to TC-49-09.
22. The owner approved the amendment of 2026-11-22 ([#100](https://github.com/sravioli/strata/issues/100)). It has tokio-postgres with deadpool-postgres, and refinery with the subcommand `strata migrate`. It also has the pipelining rule and the test case TC-49-10.

Implementation resolves these points:

- The exact SQL of the migrations, the grants and the policies, with the policy text of 0003 and 0021 (TC-3-06).
- The PgBouncer image and its pin.
- The nsjail configuration and the seccomp list for `pdftotext` ([#103](https://github.com/sravioli/strata/issues/103) completes the list).
  Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): nsjail has no release binaries. A script builds nsjail 3.6, the latest tag, from source at the pinned commit `f78475530b46d0186111a9096b30725f816b55fe`. `make setup` and CI use the same script. If the CI runner blocks the user namespaces of nsjail, CI loads an AppArmor profile that permits user namespaces for the nsjail binary only. CI does not set `kernel.apparmor_restrict_unprivileged_userns` to 0 (owner, 2026-11-22). The control group limits of 0007 work with no root only in a delegated cgroup that holds no process. Thus the worker runs each extraction in a child cgroup of the delegated cgroup of the setting `STRATA_CGROUP`. A failure of nsjail itself (exit 255) is a normal failure of the job, with no change to the revision (owner, 2026-11-22).
- The checks of Garage (0006).

These questions belong to later designs:

- Snippets, pages of results, ranking and the cost of a search in a large tenant ([#73](https://github.com/sravioli/strata/issues/73)). Amended on 2026-11-25 ([#145](https://github.com/sravioli/strata/issues/145)): tech design 0145 decides the snippets.
- Restore of an earlier revision ([#101](https://github.com/sravioli/strata/issues/101)).
- The rules for several revisions of one document in Lean ([#78](https://github.com/sravioli/strata/issues/78)).
- Metrics for each bound ([#61](https://github.com/sravioli/strata/issues/61)).
- The removal of a document ([#69](https://github.com/sravioli/strata/issues/69)). The skeleton has no `DeleteDocument` route. When it comes, the transaction that marks the document as removed must also remove its text parts. Then the search stops to find it at once (0006).

## Future possibilities

- Other formats ([#102](https://github.com/sravioli/strata/issues/102)) and PDFium ([#103](https://github.com/sravioli/strata/issues/103)) add rows to the output of `strata-extract`, with no change to the search.
- The chunks for embeddings (M2, [#70](https://github.com/sravioli/strata/issues/70)) can start from the text parts.
- The hostile-input corpus ([#48](https://github.com/sravioli/strata/issues/48)) runs on `strata-extract` after the third change.
