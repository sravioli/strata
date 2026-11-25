# revision-status Specification

## Purpose
Tells a client when a revision is ready to search, or why it failed, with one of five public statuses. A client reads one revision, or up to 100 revisions in one request, and waits for `Retry-After` between reads (tech design 0047).

## Requirements

### Requirement: The status of one revision

`GET /v1/documents/{document_id}/revisions/{revision}` SHALL use the action `ReadDocument` (0004). It SHALL answer `200` with a JSON body that holds `document_id`, `revision`, `state` and `updated_at`. `state` SHALL be one of `uploading`, `processing`, `ready`, `failed` and `superseded`. `error_class` SHALL exist only for `failed`. `no_text` SHALL exist only for `ready`. `updated_at` SHALL be the time of the last change of the public status, in RFC 3339 in UTC with whole seconds.

#### Scenario: TC-47-10 The class of a failed revision

- **GIVEN** a revision that is `failed`
- **WHEN** a client reads its status
- **THEN** `error_class` is a value from the list of the extraction design or `FileTooLarge`, and the body holds no other error text

#### Scenario: A ready revision with no text

- **GIVEN** a `Ready` revision with `no_text`
- **WHEN** a client reads its status
- **THEN** the status is `ready` with `no_text` set to `true`, and the body has no `error_class`

### Requirement: Each internal state maps to one public status

The status routes SHALL map each internal state of a revision to its public status with this table, and with no other input:

| Internal state | Public status |
|---|---|
| `AwaitingUpload` | `uploading` |
| `Uploaded` | `processing` |
| `Ready` | `ready` |
| `Failed` | `failed` |
| `Superseded` | `superseded` |

The state of the job of the revision SHALL NOT change the public status.

#### Scenario: TC-47-01 Each internal state

- **GIVEN** a revision in each internal state of the map
- **WHEN** a client reads its status
- **THEN** the public status is the status in the map

#### Scenario: TC-47-02 A parked job

- **GIVEN** a revision whose job is `Parked`
- **WHEN** a client reads its status
- **THEN** the status is `processing`

#### Scenario: TC-47-03 A newer revision is ready

- **GIVEN** revision 1 and revision 2 of one document, and revision 2 is ready
- **WHEN** a client reads revision 1
- **THEN** the status is `superseded`

#### Scenario: TC-47-13 Failed stays failed

- **GIVEN** revision 1 is `failed` and revision 2 of the same document is ready
- **WHEN** a client reads revision 1
- **THEN** the status is `failed`
- **GIVEN** revision 1 is `processing` and revision 2 is ready
- **THEN** the status of revision 1 is `superseded`

### Requirement: A status that is not final gives Retry-After

A response that holds a status of `uploading` or `processing` SHALL have the header `Retry-After` with the configured value in seconds. A batch response SHALL have `Retry-After` when one entry or more is not final. A response with only final statuses SHALL have no `Retry-After`. Each response of the two status routes SHALL have `Cache-Control: no-store`.

#### Scenario: TC-47-04 Retry-After and the final statuses

- **GIVEN** a revision that is not final
- **WHEN** a client reads its status
- **THEN** the response has `Retry-After` with the configured value
- **GIVEN** a revision that is final
- **THEN** the response has no `Retry-After`

#### Scenario: The configured value

- **GIVEN** `strata api` with `STRATA_STATUS_RETRY_AFTER` set to 5
- **WHEN** a client reads a revision that is `processing`
- **THEN** the response has `Retry-After: 5`

### Requirement: A revision of a different tenant does not exist

A revision that does not exist, a revision of a different tenant, and a path with an identifier that is not valid SHALL give the same `404`, with the same headers and the same body. The batch route SHALL omit such a revision from its list.

#### Scenario: TC-47-06 A revision of tenant B

- **GIVEN** a revision of tenant B
- **WHEN** an account of tenant A reads its status
- **THEN** the response is equal to the response for a revision that does not exist
- **WHEN** the account names it in a batch request
- **THEN** the list omits it

### Requirement: The batch status

`QUERY /v1/revisions` SHALL use the action `ReadDocument`. The body SHALL be `{"revisions": [{"document_id": …, "revision": …}, …]}` with 1 to 100 entries. A repeated entry SHALL count one time. More than 100 entries, an entry that is not valid, or another field SHALL give `400`. The response SHALL hold one entry for each revision that the API found, with the fields of the single response.

#### Scenario: TC-47-07 A batch that is not valid

- **GIVEN** a batch request with 101 identifiers, or with an identifier that is not valid
- **WHEN** a client sends it
- **THEN** the API answers `400`

#### Scenario: TC-47-08 A batch with one status that is not final

- **GIVEN** a batch request with one revision that is `processing` and one that is `ready`
- **WHEN** a client sends it
- **THEN** the response has two entries and `Retry-After`

#### Scenario: A repeated entry

- **GIVEN** a batch request that names one revision two times
- **WHEN** a client sends it
- **THEN** the response has one entry for the revision

### Requirement: The body rules of the batch

The body rules of 0047 SHALL apply to the batch route: no `Content-Type` gives `400`, a media type other than `application/json` gives `415`, and a body above 16 KB gives `413`. A body that does not arrive in 10 seconds SHALL give `408`. Each response of the route SHALL have `Accept-Query: application/json`.

#### Scenario: TC-47-15 The body rules of the batch

- **GIVEN** a batch request with no `Content-Type`
- **WHEN** a client sends it
- **THEN** the API answers `400`
- **GIVEN** a batch request with `Content-Type: text/plain`
- **THEN** the API answers `415` with `Accept-Query: application/json`
- **GIVEN** a batch request with a body larger than 16 KB
- **THEN** the API answers `413`

### Requirement: The batch path answers only QUERY

The path `/v1/revisions` SHALL answer each method other than `QUERY` with `405` and the header `Allow: QUERY`.

#### Scenario: A GET to the batch path

- **WHEN** a client sends a `GET` to `/v1/revisions`
- **THEN** the API answers `405` with `Allow: QUERY`

### Requirement: The status of an upload from start to search

A client that reads the `Location` of a finish until the status is final SHALL see `ready` for a PDF with text, and a search SHALL then find the document.

#### Scenario: TC-47-11 The Location until the final status

- **GIVEN** the walking skeleton locally
- **WHEN** a client uploads a document and reads the `Location` URL until the status is final
- **THEN** the status is `ready`, and a search finds the document
