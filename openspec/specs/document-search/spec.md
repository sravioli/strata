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

### Requirement: The search gives one hit for each matching location

The search SHALL parse `q` with `websearch_to_tsquery('simple', q)`. It SHALL rank each matching text part with `ts_rank`, and keep the best part of each location (`document_id`, `location_kind`, `location_number`). With the same rank, it SHALL keep the part with the lowest number. It SHALL give at most `limit` hits, best first. With the same score, it SHALL order the hits by `document_id`, then `location_kind`, then `location_number`. Each hit SHALL hold `document_id`, `revision`, `score`, the `location` (`kind` and `number`) and `text`. The response SHALL hold no count of the matches.

#### Scenario: The page of the best match

- **GIVEN** a document of two pages, and the word `quartz` two times on page 2 and one time on page 1
- **WHEN** an account searches for `quartz`
- **THEN** the response has two hits of the document
- **AND** the first hit has `location` `{"kind": "page", "number": 2}`

#### Scenario: A query that PostgreSQL cannot parse as tsquery syntax

- **WHEN** an account searches for `"quartz` with one quote and `-` alone
- **THEN** the API answers `200` with a list, and no error

#### Scenario: TC-145-01 Three pages of one document

- **GIVEN** a document of tenant A with the word `quartz` three times on page 2, one time on page 1 and two times on page 3
- **WHEN** A searches for `quartz`
- **THEN** the response has three hits of the document, in the order page 2, page 3, page 1
- **AND** each score is less than or equal to the score before it

#### Scenario: TC-145-02 One hit for a page of four parts

- **GIVEN** a page of 200 KB in four parts, with the word `obsidian` in parts 2 and 4
- **WHEN** an account searches for `obsidian`
- **THEN** the response has one hit for that page

#### Scenario: TC-145-03 The limit counts hits

- **GIVEN** a document with the word `basalt` on five pages, and a second document with the word on one page
- **WHEN** an account searches for `basalt` with `limit` 3
- **THEN** the response has the three hits with the best scores, of the two documents

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

### Requirement: Each hit has a snippet as segments

The field `text` of a hit SHALL be a list of segments, from one fragment of 15 to 35 words of the best part of the location, around the best match. A segment SHALL be `{"t": …}` for text, or `{"t": …, "match": true}` for a matched word. No segment SHALL be empty. A location with fewer than 15 words SHALL give all its words. The API SHALL make a snippet only for the hits of the response.

#### Scenario: TC-145-04 The segments of a short page

- **GIVEN** a page with the text `alpha bravo charlie`
- **WHEN** an account searches for `bravo`
- **THEN** the hit has three segments: `"alpha "`, then `"bravo"` with `match` set to `true`, then `" charlie"`

#### Scenario: TC-145-06 The snippet of a long page

- **GIVEN** a page of 1,000 words with the word `granite` in the middle
- **WHEN** an account searches for `granite`
- **THEN** the snippet has at most 35 words
- **AND** one of its segments is `granite` with `match`

### Requirement: The text of a document cannot make a mark

The API SHALL remove the characters U+0002 and U+0003 from the text of a part before it makes the snippet, so a document cannot fake a match. A snippet SHALL hold no text in the form of an HTML or XML tag. The other characters of the text SHALL stay as they are.

#### Scenario: TC-145-05 Tags and control characters in the text

- **GIVEN** a page with the text `<b>alpha</b> bravo`, and the characters U+0002 and U+0003 around the word `charlie`
- **WHEN** an account searches for `bravo`
- **THEN** the only segment with `match` is `bravo`
- **AND** the word `alpha` is in a segment with no tag
- **AND** no segment holds U+0002 or U+0003
