Issue: #145

# Proposal

## Why

`QUERY /v1/search` gives one result for each document, with no text (tech design 0049). A document with the word on ten pages shows one page only, and a person must open each document to see why it matched. Tech design 0145 changes each result to a hit: one matching location, with a snippet of the text around the matched words. The frontend for demonstrations needs the text to show each hit (task 4.2 of `demo-frontend`).

## What Changes

- The search keeps the best part of each location (`document_id`, `location_kind`, `location_number`), not of each document. It gives at most `limit` hits, best first. With the same score, it orders by `document_id`, then `location_kind`, then `location_number`.
- Only the keys and the ranks of the parts go through the limit. Then the query reads the text of the kept part of each hit, and runs `ts_headline` on it, with 15 to 35 words in one fragment.
- The query removes U+0002 and U+0003 from the text of the part before `ts_headline` reads it. These two characters are the start mark and the stop mark of a matched word.
- **BREAKING**: each result gets the field `text`, a list of segments `{"t": …}` and `{"t": …, "match": true}`. A document can give more than one result. The route, its body, its limits and its errors do not change. The API has no client outside this repository.
- The API splits the output of `ts_headline` at the marks into segments, and leaves out each empty segment.
- `dev/strata.hurl`, the L1 tests and the L2 tests of the search read hits, not documents.
- The page `/search` of `web/` shows one row for each hit, with its snippet. It sets `textContent`, and it shows a segment with `match` in bold. TC-65-07 checks the snippet in the browser.

Not in this change:

- Ranking other than `ts_rank`, the hybrid search, pages of results, a limit of hits for each document, and the cost of a search in a large tenant ([#73](https://github.com/sravioli/strata/issues/73)).
- A snippet that keeps the text in the form of an HTML or XML tag ([#148](https://github.com/sravioli/strata/issues/148)).
- More than one fragment for each hit.

Tech design: [docs/tech-design/0145-search-hit.md](../../../docs/tech-design/0145-search-hit.md), with the amendments of 0049 and ADR 0011.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `document-search`: a result is a hit of one location, up to `limit`, and each hit has a snippet as segments. The rule "the response holds no text of a document" goes away for the text of the hits. The rule "no count of the matches" stays.
- `demo-frontend`: the page `/search` shows each hit with its snippet as text. The main spec of this capability comes with the archive of `demo-frontend` (#150).

## Impact

- `crates/strata/src/db/search.rs`: the query and the struct `Hit`, which gets the text of the snippet.
- `crates/strata/src/search.rs`: the split into segments, with L0 tests, and the field `text` of each result.
- `crates/strata-tests/tests/search.rs`: the new L1 tests TC-145-01 to 06, and the scenario "The page of the best match".
- `crates/strata-tests/tests/l2.rs` and `dev/strata.hurl`: they read hits.
- `web/src/pages/search.astro` and `web/tests/flow.spec.ts`.
- No schema change, no migration, no new setting and no new dependency. To roll back, revert the change.
