# Spec Delta

## Purpose

Lets an account put the bytes of a document into the object storage of ADR 0005 with a signed URL, and tells the API when the bytes are there. The upload creates the revision and its job (tech designs 0006 and 0049).

## ADDED Requirements

### Requirement: The start of an upload creates a revision and a signed URL

`POST /v1/documents` SHALL create a document and its revision 1. `POST /v1/documents/{document_id}/revisions` SHALL create the next revision of a document of the tenant of the caller. Each SHALL set the revision to `AwaitingUpload` and answer `201` with `document_id`, `revision`, a signed PUT URL and the expiry of the URL. The URL SHALL permit a PUT of the key of that revision only, and SHALL expire after 15 minutes.

#### Scenario: Start of a new document

- **GIVEN** an active account of tenant A
- **WHEN** the account sends `POST /v1/documents` with a valid body
- **THEN** the API answers `201` with a new `document_id`, `revision` 1, a signed PUT URL and its expiry
- **AND** the revision is in `AwaitingUpload`, and one row of `upload_slots` exists for it

#### Scenario: Start of the next revision

- **GIVEN** a document of tenant A with revision 1
- **WHEN** an account of A sends `POST /v1/documents/{document_id}/revisions`
- **THEN** the API answers `201` with the same `document_id` and `revision` 2

#### Scenario: A document of a different tenant

- **GIVEN** a document of tenant B
- **WHEN** an account of A sends `POST /v1/documents/{document_id}/revisions` with the `document_id` of B
- **THEN** the API answers `404`, the same answer as for a `document_id` that does not exist
- **AND** the document of B gets no new revision

#### Scenario: The key of the URL

- **WHEN** the API signs the URL of revision `r` of document `d` of tenant `t`
- **THEN** the key of the URL is `tenants/{t}/documents/{d}/revisions/{r}/original`

#### Scenario: TC-49-04 Two starts at the same time get two numbers

- **GIVEN** two starts of an upload for the same `document_id` at the same time
- **WHEN** both complete
- **THEN** they get two different revision numbers, one after the other
- **PENDING** Task 5.4 adds the test.

### Requirement: The API checks the start body

The start body SHALL be a JSON object with `file_name`, `content_type` and `size`. `file_name` and `content_type` SHALL each have 1 to 1,024 characters. `size` SHALL be an integer from 0 to 104,857,600 (100 MB). A body with no `Content-Type` SHALL get `400`. A media type other than `application/json` SHALL get `415`. A body above 16,384 bytes SHALL get `413`. A size above 100 MB SHALL get `413`. Each other body that breaks a rule SHALL get `400`. A rejected start SHALL create no row.

#### Scenario: A size above the maximum

- **WHEN** a client starts an upload with `size` 104,857,601
- **THEN** the API answers `413`, and no document, revision or row of `upload_slots` exists for it

#### Scenario: A size of 0 is valid

- **WHEN** a client starts an upload with `size` 0
- **THEN** the API answers `201`

#### Scenario: Values that break a rule

- **WHEN** a client sends a body with no `file_name`, an empty `content_type`, a `file_name` of 1,025 characters, or a `size` of -1
- **THEN** the API answers `400` to each, and creates no row

#### Scenario: The rules of the body itself

- **WHEN** a client sends a start with no `Content-Type`, with `Content-Type: text/plain`, or with a body of 16,385 bytes
- **THEN** the API answers `400`, `415` and `413`

### Requirement: The start stops at the bounds

Before it creates a row, the start SHALL count the `Waiting` jobs and the rows of `upload_slots` from the last 2 hours. At 20,000 `Waiting` jobs or at 5,000 rows, the API SHALL answer `503` with `Retry-After: 60`, create no row, and log the rejection with the name of the bound.

#### Scenario: TC-5-06 A full queue rejects the start

- **GIVEN** a count of `Waiting` jobs at the bound
- **WHEN** a client starts an upload
- **THEN** the API answers `503` with `Retry-After: 60`, and no revision and no job exist for the upload
- **PENDING** Task 5.5 adds the test.

#### Scenario: Too many unfinished uploads reject the start

- **GIVEN** 5,000 rows of `upload_slots` from the last 2 hours
- **WHEN** a client starts an upload
- **THEN** the API answers `503` with `Retry-After: 60`, and no document or revision exists for the upload

#### Scenario: Old rows do not count

- **GIVEN** 5,000 rows of `upload_slots` that are older than 2 hours
- **WHEN** a client starts an upload
- **THEN** the API answers `201`

### Requirement: The finish of an upload gives one answer for each state

`POST /v1/documents/{document_id}/revisions/{revision}/finish` SHALL lock the revision, read the size of its object, and give the answer of its state. Each `202` SHALL have `Location: /v1/documents/{document_id}/revisions/{revision}`. A revision of a different tenant SHALL get `404`. Only a revision that leaves `AwaitingUpload` SHALL lose its row of `upload_slots`.

#### Scenario: A finish with the object

- **GIVEN** a revision in `AwaitingUpload` with an object of 100 MB or less
- **WHEN** the client finishes the upload
- **THEN** the API answers `202` with the `Location` of the revision
- **AND** the revision is `Uploaded`, one job exists for it, and its row of `upload_slots` is removed

#### Scenario: A finish of a revision in a later state

- **GIVEN** a revision in `Uploaded`, `Ready` or `Superseded`, or `Failed` with a class other than `FileTooLarge`
- **WHEN** the client finishes the upload
- **THEN** the API answers `202` with the `Location` of the revision
- **AND** the revision does not change, and no new job exists

#### Scenario: TC-6-03 A file above the maximum

- **GIVEN** an upload of a file above the maximum size
- **WHEN** the client finishes the upload
- **THEN** the API answers `413`, the object is removed, the revision is `Failed` with `FileTooLarge`, and no job exists
- **AND** the row of `upload_slots` of the revision is removed
- **WHEN** the client finishes the upload a second time
- **THEN** the API answers `413` again, and no job exists
- **PENDING** Task 5.6 adds the test.

#### Scenario: TC-6-04 A finish with no object

- **GIVEN** an upload that the client started but did not send
- **WHEN** the client finishes the upload
- **THEN** the API answers `409`, the revision stays in `AwaitingUpload`, and no job exists
- **PENDING** Task 5.6 adds the test.

#### Scenario: TC-6-05 A second finish changes nothing

- **GIVEN** an upload that the client finished
- **WHEN** the client finishes it a second time
- **THEN** the API gives the same answer, and one job exists
- **PENDING** Task 5.6 adds the test.

#### Scenario: TC-47-09 Each finish gives the same Location

- **GIVEN** an upload that the client finished
- **WHEN** the client finishes it a second time
- **THEN** each answer is `202` with the same `Location`
- **PENDING** Task 5.6 adds the test.

#### Scenario: TC-49-09 Two finishes at the same time create one job

- **GIVEN** a revision in `AwaitingUpload` with its object in the storage
- **WHEN** the client sends two finishes at the same time, and then a third
- **THEN** the revision is `Uploaded`, it has exactly one job, and each finish answers `202` with the same `Location`
- **PENDING** Task 5.6 adds the test.

#### Scenario: A finish of a revision of a different tenant

- **GIVEN** a revision of tenant B in `AwaitingUpload` with its object
- **WHEN** an account of A finishes it
- **THEN** the API answers `404`, and the revision of B stays in `AwaitingUpload`

### Requirement: Garage accepts the signed URLs

The storage SHALL accept a PUT with a signed URL of the API before the URL expires, and SHALL refuse a PUT to a different key or after the expiry. The storage SHALL check the expiry only when the request arrives, so that a transfer that starts in time completes.

#### Scenario: TC-6-01 A URL for one key cannot write a different key

- **GIVEN** a signed PUT URL for one key
- **WHEN** a client uses it to write a different key
- **THEN** the storage refuses the write
- **PENDING** Task 4.2 adds the test.

#### Scenario: TC-6-02 An expired URL cannot write

- **GIVEN** a signed PUT URL
- **WHEN** a client uses it after it expires
- **THEN** the storage refuses the write
- **PENDING** Task 4.2 adds the test.

#### Scenario: A slow transfer that starts in time completes

- **GIVEN** a signed PUT URL with an expiry of a few seconds
- **WHEN** a client starts the PUT before the expiry, and sends the last byte after the expiry
- **THEN** the storage keeps the object

### Requirement: The API reads its storage settings from the environment

`strata api` SHALL read the storage settings only from the variables `AWS_ENDPOINT`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_DEFAULT_REGION`, `AWS_BUCKET_NAME` and `AWS_ALLOW_HTTP`. With no access key or no secret key, it SHALL stop before it serves a request, with an error that names the variable. It SHALL not ask a metadata service for credentials. When `AWS_ENDPOINT` starts with `http://` and `AWS_ALLOW_HTTP` is not `true`, it SHALL stop before it serves a request, with an error that names `AWS_ALLOW_HTTP`.

#### Scenario: No secret key

- **GIVEN** the storage variables with no `AWS_SECRET_ACCESS_KEY`
- **WHEN** an operator starts `strata api`
- **THEN** the process stops with an error that names `AWS_SECRET_ACCESS_KEY`

#### Scenario: An http endpoint needs AWS_ALLOW_HTTP

- **GIVEN** an `AWS_ENDPOINT` that starts with `http://` and no `AWS_ALLOW_HTTP`
- **WHEN** an operator starts `strata api`
- **THEN** the process stops with an error that names `AWS_ALLOW_HTTP`

### Requirement: The API removes old rows of upload_slots

`strata api` SHALL remove the rows of `upload_slots` that are older than 2 hours, each 60 seconds, at most 1,000 rows in each batch. It SHALL not change a revision or an object.

#### Scenario: Old rows go and new rows stay

- **GIVEN** a row of `upload_slots` from 3 hours ago and one from 1 hour ago
- **WHEN** the cleanup runs
- **THEN** the first row is removed and the second row stays
- **AND** the revision of each row does not change
