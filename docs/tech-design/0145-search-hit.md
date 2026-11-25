- Issue: [#145](https://github.com/sravioli/strata/issues/145)
- Status: draft
- Start date: 2026-11-25
- Decided by:

# What a search hit is: one location, with a snippet

## Summary

Today `QUERY /v1/search` gives one result for each document: the best part of the document, with no text (tech design 0049). This design changes the result to a **hit**. A hit is one matching location of a document: a page, a sheet or a slide. The search gives each matching location as a hit, up to `limit`, best first. Each hit has a short **snippet** of the text around the matched words. The snippet is a list of segments, and each matched word is a segment with a mark, so a client shows it with no HTML. The route, its body and its other fields do not change.

## Motivation

A person who searches wants to see each place where the words occur, and the words around them. With one result for each document, a document with the word on ten pages shows one page only. With no text, the person must open each document to see why it matched. The frontend for demonstrations (tech design 0065, #65) must show "each hit with its text", and the API gives no text. The owner chose to add the text to the API (2026-11-25).

## Guide-level explanation

Two new terms:

- **Hit**: one location of a document whose text matches the query. A location is one page of a PDF, and later one sheet or one slide. A document with the word on five pages gives five hits.
- **Snippet**: about 15 to 35 words of the text of a hit, around the best match. It is a list of **segments**. A segment is a piece of the text. A segment with `"match": true` is a word of the query.

An example. Tenant `acme` has one PDF. Page 2 holds "bravo page two" and page 5 holds "the bravo team met on monday". An account of `acme` sends:

```json
{"q": "bravo", "limit": 20}
```

The API answers:

```json
{"results": [
  {"document_id": "01a2…", "revision": 1, "score": 0.0608,
   "location": {"kind": "page", "number": 2},
   "text": [{"t": "bravo", "match": true}, {"t": " page two"}]},
  {"document_id": "01a2…", "revision": 1, "score": 0.0608,
   "location": {"kind": "page", "number": 5},
   "text": [{"t": "the "}, {"t": "bravo", "match": true}, {"t": " team met on monday"}]}
]}
```

Both pages hold `bravo` one time, so both hits have the same score, and page 2 comes first. A client shows each hit as one row. It joins the `t` of the segments, and it shows a segment with `match` in bold, as text. It does not read the snippet as HTML.

`limit` counts hits, not documents. A long document can fill all the hits of an answer. The order is by score only, and the owner chose that (2026-11-25).

## Reference-level explanation

### The hits

The worker keeps the text of each location in parts of at most 64 KB (0049). A location of 200 KB has four parts. A part is a limit of the storage, and a reader does not see it. So a hit is one location, not one part:

1. The search finds each part of the tenant of the caller that matches `websearch_to_tsquery('simple', q)`, as today.
2. It ranks each part with `ts_rank`, as today.
3. For each location (`document_id`, `location_kind`, `location_number`), it keeps the part with the best rank. With the same rank, it keeps the part with the lowest `part`. The score of the hit is the rank of that part.
4. It orders the hits by score, best first. With the same score, it orders by `document_id`, then `location_kind`, then `location_number`.
5. It keeps the first `limit` hits.
6. It makes a snippet for each of these hits only, from the text of the kept part.

`text_parts` holds the text of `Ready` revisions only (0049). So each location has one revision, and a search finds the current text only, as today.

The query, in outline:

```sql
with best as (
  select distinct on (p.document_id, p.location_kind, p.location_number)
         p.document_id, p.revision, p.part, p.location_kind, p.location_number,
         ts_rank(p.tsv, q.query) as rank
  from text_parts p, websearch_to_tsquery('simple', $2) as q (query)
  where p.tenant_id = $1 and p.tsv @@ q.query
  order by p.document_id, p.location_kind, p.location_number, rank desc, p.part
), top as (
  select * from best
  order by rank desc, document_id, location_kind, location_number
  limit $3
)
select t.document_id, t.revision, t.location_kind, t.location_number, t.rank,
       ts_headline('simple', translate(p.part_text, chr(2) || chr(3), ''),
                   websearch_to_tsquery('simple', $2),
                   'StartSel=' || chr(2) || ', StopSel=' || chr(3) || ', MinWords=15, MaxWords=35')
from top t
join text_parts p on (p.tenant_id, p.document_id, p.revision, p.part)
                   = ($1, t.document_id, t.revision, t.part)
order by t.rank desc, t.document_id, t.location_kind, t.location_number
```

The steps before the limit carry only the key of a part and its rank, as the query of today does. The text of a part comes back only for the hits of the answer.

`chr(2)` and `chr(3)` are the control characters U+0002 and U+0003. PostgreSQL 19 accepts them as `StartSel` and `StopSel` (checked on the image of `compose.yaml`, 2026-11-25).

### The snippet

`ts_headline` gives the text of the fragment, with a start mark and a stop mark around each matched word. The marks of this design are U+0002 (start) and U+0003 (stop). The query removes both characters from the text of the part before `ts_headline` reads it. So the text of a document cannot make a mark.

The API splits the output of `ts_headline` at the marks:

- The text outside the marks gives segments `{"t": "…"}`.
- The text between a start mark and a stop mark gives a segment `{"t": "…", "match": true}`.
- The API leaves out an empty segment.

The parser of PostgreSQL reads text in the form of an HTML or XML tag as a tag. `ts_headline` leaves each tag out of the snippet. On PostgreSQL 19:

- `bravo <script>alert(1)</script> end` gives `bravo`, `alert(1)` and `end`, with no tags.
- `if x < y and a<b then bravo` stays as it is, because no part of it has the form of a tag.
- `see <http://example.com> bravo` stays as it is.
- `&` and quotes stay as they are.

So a snippet can lack the text of a tag, but it holds no tag. The other characters of the text stay as they are. The API escapes nothing, because JSON carries them.

The fragment has 15 to 35 words, the default of `ts_headline`, in one fragment. A location with fewer than 15 words gives all its words. A query that matches by a negation only, for example `-quartz`, gives the start of the text, with no segment with `match`.

### The response

Each result of `QUERY /v1/search` holds:

| Field | Value |
|---|---|
| `document_id` | The document of the hit, as today |
| `revision` | The revision of the text, as today |
| `score` | The rank of the best part of the location, as today |
| `location` | `{"kind": …, "number": …}` of the hit, as today |
| `text` | New: the segments of the snippet |

The response holds no count of the matches, as today. The route, its method, its body, its limits and its errors do not change. The statement timeout of 2 seconds (0049) covers the whole query, `ts_headline` included. A timeout gives `503` with `Retry-After: 1`, as today.

### The clients

The route changes in place, with no new version. The API has no clients outside this repository. These change in the same change:

- `dev/strata.hurl`: its search steps read the hits.
- The L1 test "The page of the best match" and the L2 tests of the search.
- The page `/search` of `web/`: one row for each hit, with its snippet. This closes task 4.2 of the OpenSpec change `demo-frontend`.

### This design and #73

Issue #73 designs the search path of M2. It decides the hybrid search, ranking other than `ts_rank`, pages of results and the cost of a search in a large tenant. This design decides only the shape of a hit for the lexical search of today. #73 keeps each other point, and it can change the ranking under the same shape of a hit.

## Proof obligations

N/A. This design changes the query and the response of the search route. It adds no function to `crates/core` and no domain rule. L0 and L1 tests cover it.

## Impacts

### Infrastructure

- No new service and no new setting.
- `ts_headline` reads the whole text of a part. It runs only for the hits of the answer: at most 100 parts of at most 64 KB. The statement timeout of 2 seconds stays the limit of a search.
- An answer grows by the snippets: at most 100 snippets of 35 words.
- To roll back: revert the change of the query and the response. No data changes.

### Security and tenant isolation

- The search reads the text parts of the tenant of the caller only, as today: the policy of `text_parts` (0003) and the `tenant_id` in the query. A snippet is text of the tenant of the caller.
- The rule of 0049 "the response holds no text of a document" goes away for the text of the hits. The rule "no count of the matches" stays, so the response tells nothing about the text of a different tenant (TC-3-10).
- The text of a document comes from the uploader, so the API does not trust it. The marks are control characters that the query removes from the text first, so a document cannot fake a match. A segment is plain text in JSON. A client must show it as text. `web/` sets `textContent`, not `innerHTML`.
- The query string `q` goes to PostgreSQL as a parameter, as today.

### Data and migration

- No schema change and no migration.
- The text of `text_parts` does not change.
- The new code and the old code use the same tables. A client of the old shape reads the same fields, with the new field `text`. But it can get more than one result for a document.

## Test cases

L0 tests cover the split of the output of `ts_headline` into segments. The cases are: no mark, one mark, and marks at the start and at the end. Also two marks next to each other, and a mark with no stop. L1 tests send searches to the router.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-145-01 | L1 | Given a document of tenant A with the word `quartz` three times on page 2, one time on page 1 and two times on page 3. When A searches for `quartz`. Then the response has three hits of the document, in the order page 2, page 3, page 1. Each score is less than or equal to the score before it. | | pending |
| TC-145-02 | L1 | Given a page of 200 KB in four parts, with the word `obsidian` in parts 2 and 4. When an account searches for `obsidian`. Then the response has one hit for that page. | | pending |
| TC-145-03 | L1 | Given a document with the word `basalt` on five pages, and a second document with the word on one page. When an account searches for `basalt` with `limit` 3. Then the response has the three hits with the best scores, of the two documents. | | pending |
| TC-145-04 | L1 | Given a page with the text `alpha bravo charlie`. When an account searches for `bravo`. Then the hit has three segments: `"alpha "`, then `"bravo"` with `match` set to `true`, then `" charlie"`. | | pending |
| TC-145-05 | L1 | Given a page with the text `<b>alpha</b> bravo`, and the characters U+0002 and U+0003 around the word `charlie`. When an account searches for `bravo`. Then the only segment with `match` is `bravo`, the word `alpha` is in a segment with no tag, and no segment holds U+0002 or U+0003. | | pending |
| TC-145-06 | L1 | Given a page of 1,000 words with the word `granite` in the middle. When an account searches for `granite`. Then the snippet has at most 35 words, and one of its segments is `granite` with `match`. | | pending |

TC-49-02, TC-49-05, TC-49-06, TC-3-10, TC-4-01 and TC-4-09 keep their meaning. Their tests read hits, not documents. TC-65-07 shows the snippet in the browser.

## Drawbacks

- A long document with the word on many pages can fill an answer, so other documents do not show. The owner chose the order by score only (2026-11-25).
- `ts_headline` adds time to each search, at most 100 times the time for one part.
- The snippet of a location comes from its best part only. A match in a different part of the same location does not show in the snippet.
- A snippet lacks the text that has the form of an HTML or XML tag, because `ts_headline` leaves tags out ([#148](https://github.com/sravioli/strata/issues/148)).

## Rationale and alternatives

**One hit for each part.** It needs no step for each location. But one location of 200 KB then gives up to four hits for the same page, and the reader cannot tell them apart.

**One result for each document, with a list of its locations.** Acrobat and Google Drive show results like this. But then `limit` counts documents, and the owner wants the hits up to `limit`, ranked best first (2026-11-25).

**The whole text of the part.** It needs no `ts_headline`. But a part holds up to 64 KB, and 100 hits give an answer of up to 6.4 MB.

**A snippet with HTML marks**, such as `<b>bravo</b>`, the default of `ts_headline`. A document can hold `<b>` itself, so a client cannot tell a mark of the API from the text. A client that reads it as HTML can also show markup from the document. Segments carry the marks as data.

**Offsets of the matches in plain text.** A client must then count characters, and JavaScript counts UTF-16 units, not Unicode characters. Segments need no count.

**A limit of hits for each document**, as web search engines do for each site. The owner chose no such limit for now (2026-11-25). #73 can add it.

**Wait for #73.** The demonstration needs the text now, and #73 is a large design of M2.

## Prior art

- Web search engines give one result for each page, with a snippet of the best match, and they limit the results of one site.
- Elasticsearch and OpenSearch keep chunks of a document as hits, and `collapse` or `inner_hits` groups them under the parent. Their highlighter gives HTML marks such as `<em>`.
- PostgreSQL documents `ts_headline` with the options `StartSel`, `StopSel`, `MinWords`, `MaxWords` and `MaxFragments` ("Highlighting Results" of the text search chapter). It warns that `ts_headline` reads the whole document and is slow, so a query uses it only on the rows of the answer.

## Unresolved questions

The owner answered the questions of #145 on 2026-11-25:

1. #145 decides the shape of a hit for the search of today. #73 keeps the ranking, the hybrid search, the pages of results and the cost.
2. A hit is one matching location, up to `limit`, best first.
3. Each hit has a snippet from `ts_headline`, only for the hits of the answer. "No count of the matches" stays.
4. The route changes in place. It keeps its fields and gets the field `text`.
5. The page `/search` of `web/` changes in the same change.
6. The snippet is a list of segments, with `match` on each matched word.
7. A snippet is one fragment of 15 to 35 words.
8. No limit of hits for each document, for now.
9. This design amends 0049 and ADR 0011 (walking skeleton). The pull request of this design changes their text at each place, with an amendment note. At acceptance, `GLOSSARY.md` gets the terms hit and snippet.

The owner accepted on 2026-11-25 that a snippet lacks the text of each HTML or XML tag (see "The snippet"). A snippet with the exact text is a later design: [#148](https://github.com/sravioli/strata/issues/148).

During implementation:

- Whether `ts_headline` with `MaxFragments` 0 (the default) or 1 gives the better fragment for a long part. The size stays 15 to 35 words.
- The time of two searches against the statement timeout of 2 seconds: one with 100 hits of 64 KB, and one with many matching locations.

## Future possibilities

- More than one fragment for each hit (`MaxFragments`).
- A limit of hits for each document, and pages of results (#73).
- The hit of a sheet or a slide, when the worker extracts more formats (#102).
- A snippet that keeps the text in the form of a tag (#148).
