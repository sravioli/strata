# document-search Specification

## Purpose
Finds the locations of the documents of the caller's tenant whose text matches the words of a query, best match first, each with a snippet of the text around the matched words. The search reads only the text parts of the caller's tenant (tech designs 0049 and 0145).

## Requirements

### Requirement: The search route takes one query

`QUERY /v1/search` SHALL take a JSON body with `q` and an optional `limit`. `q` SHALL have 1 to 1,000 characters. `limit` SHALL be an integer from 1 to 100, and 20 when it is absent. Each other value, and each other field, SHALL give `400`. The route SHALL use the action `Search` (0004). The body rules of 0047 SHALL apply: no `Content-Type` gives `400`, a media type other than `application/json` gives `415`, and a body above 16 KB gives `413`.

#### Scenario: A body with the default limit

- **GIVEN** 25 documents of the tenant with the word `quartz`
- **WHEN** an account sends `{"q": "quartz"}`
- **THEN** the response has 20 results

#### Scenario: A body of another media type

- **WHEN** a client sends a search body with `Content-Type: text/plain`
- **THEN** the API answers `415`

#### Scenario: TC-49-07 A body that is not valid

- **GIVEN** a search body with no `q`, a body with a `limit` of 101, and a body above 16 KB
- **WHEN** the client sends each one
- **THEN** the API answers `400`, `400` and `413`

### Requirement: The search path answers only QUERY

The path `/v1/search` SHALL answer each method other than `QUERY` with `405` and the header `Allow: QUERY`. The API SHALL check the method before it reads the body or the token.

#### Scenario: TC-49-08 A GET and a POST

- **GIVEN** a `GET` and a `POST` to `/v1/search`
- **WHEN** the API gets them
- **THEN** it answers `405` with the header `Allow: QUERY`

### Requirement: The search ranks the best part of each document

The search SHALL parse `q` with `websearch_to_tsquery('simple', q)`. It SHALL rank each matching text part with `ts_rank`, keep the best part of each document, and give at most `limit` documents, best first. Each result SHALL hold `document_id`, `revision`, `score` and the `location` of the best part (`kind` and `number`). The response SHALL hold no text of a document and no count of the matches.

#### Scenario: The page of the best match

- **GIVEN** a document of two pages, and the word `quartz` two times on page 2 and one time on page 1
- **WHEN** an account searches for `quartz`
- **THEN** the result has `location` `{"kind": "page", "number": 2}`

#### Scenario: A query that PostgreSQL cannot parse as tsquery syntax

- **WHEN** an account searches for `"quartz` with one quote and `-` alone
- **THEN** the API answers `200` with a list, and no error

### Requirement: The search reads only the scope of the caller

The search SHALL read the text parts of the tenant of the caller only. A document of a different tenant SHALL change no result and no count in the response. Each account of a tenant SHALL find each document of the tenant.

#### Scenario: TC-49-05 Two tenants with the same word

- **GIVEN** documents of tenants A and B with the same word
- **WHEN** A searches for the word
- **THEN** each result is a document of A

#### Scenario: TC-4-01 Two accounts of one tenant

- **GIVEN** accounts Anna and Bruno of one tenant, and a document that Anna uploaded
- **WHEN** Bruno searches for the document and reads it
- **THEN** Bruno finds it and reads it

#### Scenario: TC-3-10 A search for the word of a different tenant

- **GIVEN** tenants A and B each own a document that contains a word no other document contains
- **WHEN** an account of A searches for the word of B through the public API
- **THEN** the response contains no result and no count that reveals the document of B

#### Scenario: TC-4-09 Two accounts of one tenant and one of a different tenant

- **GIVEN** two active accounts of one tenant and one account of a different tenant
- **WHEN** each account uploads a document and then searches through the public API
- **THEN** the two accounts of one tenant see the same two documents
- **AND** the third account sees only its own document

### Requirement: The search finds only the current text

A search SHALL find the text of `Ready` revisions only. It SHALL NOT find the text of a `Superseded` revision.

#### Scenario: TC-47-17 A newer revision with no text

- **GIVEN** revision 1 of a document with text is `ready`
- **WHEN** revision 2, a scanned PDF with no text, becomes ready
- **THEN** revision 2 is `ready` with `no_text` set to `true`
- **AND** revision 1 is `superseded`, and a search does not find its text

### Requirement: The search stops at its time limit

The search SHALL run with a statement timeout of 2 seconds (0049). A search that reaches the timeout SHALL give `503` with `Retry-After: 1` and no body (S4 of 0002). The API SHALL log the timeout with no text of the query.

#### Scenario: A search at the time limit

- **GIVEN** a statement timeout that the test sets lower than the time of the search
- **WHEN** an account searches
- **THEN** the API answers `503` with `Retry-After: 1`
