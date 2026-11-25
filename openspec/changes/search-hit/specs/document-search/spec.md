## RENAMED Requirements

- FROM: `### Requirement: The search ranks the best part of each document`
- TO: `### Requirement: The search gives one hit for each matching location`

## MODIFIED Requirements

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

## ADDED Requirements

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
