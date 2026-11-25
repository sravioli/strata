# Tasks

## 1. The hits of the query

- [ ] 1.1 In `crates/strata/src/db/search.rs`, change the query to the outline of 0145. `distinct on (document_id, location_kind, location_number)` keeps the best part of each location, and the lowest `part` with the same rank. Order by rank, then `document_id`, `location_kind` and `location_number`, and keep the first `limit` hits. Only the keys and the ranks go through the limit. Keep the pipelining rule and the statement timeout of 0049. Verify: the L1 tests TC-145-01, TC-145-02 and TC-145-03 pass in `crates/strata-tests/tests/search.rs`. The scenario "The page of the best match" passes with its two hits. Remove the pending markers of TC-145-01 to 03.
- [ ] 1.2 Update the tests of the search that count documents: `results` in `crates/strata-tests/tests/search.rs`, and TC-49-02, TC-49-05, TC-49-06, TC-4-01 and TC-47-17. Each test keeps its meaning and reads hits. Verify: `make l1` passes.

## 2. The snippet

- [ ] 2.1 Join the hits of 1.1 back to `text_parts`. Run `ts_headline('simple', …)` on the text of the kept part only, after `translate` removes U+0002 and U+0003. Use `StartSel` U+0002, `StopSel` U+0003, `MinWords=15` and `MaxWords=35`. `Hit` gets the output of `ts_headline`. Compare `MaxFragments` 0 (the default) and 1 on a part of 64 KB with the word at three places. Keep the option that gives the fragment around the best match, and write the result in the pull request. Verify: TC-145-06 passes.
- [ ] 2.2 In `crates/strata/src/search.rs`, add one pure function that splits the output of `ts_headline` at the marks into segments `{"t": …}` and `{"t": …, "match": true}`, and leaves out each empty segment. A start mark with no stop mark makes the rest a match. A stop mark with no start mark is text. Neither mark stays in a segment. Add the field `text` to each result. Update the doc comments of both modules, which name the best part of each document. Verify: L0 tests of no mark, one mark, marks at the start and at the end, two marks next to each other, and a mark with no stop pass. The L1 tests TC-145-04 and TC-145-05 pass. Remove the pending markers of TC-145-04 to 06.
- [ ] 2.3 Measure two searches locally against the statement timeout of 2 seconds: one with 100 hits, each a part of 64 KB, and one with 10,000 matching locations and `limit` 100. Write the times in the pull request. Verify: both searches end in less than 2 seconds. If a search does not, stop and report it to the owner before the pull request.

## 3. The clients

- [ ] 3.1 Update `dev/strata.hurl` and `crates/strata-tests/tests/l2.rs`. They read the `document_id` of the hits, and each hit of a search for the unique word has a segment with `match`. Verify: `make l2` and `make hurl` pass after `make dev`. TC-49-01, TC-3-10 and TC-4-09 keep their meaning.
- [ ] 3.2 In `web/src/pages/search.astro`, show one row for each hit with its location and its snippet. Put the text of each segment in a text node or in `textContent`, and a segment with `match` in a `<strong>`. Never use `innerHTML`. In `web/tests/flow.spec.ts`, check that the hit of `alice@acme` shows the searched word in a `<strong>`. Verify: `npm run check` and `npm run build` in `web/` pass. By hand after `make dev` and `make web`, the scenarios "A hit with a matched word" and "A snippet that holds markup" pass, and TC-65-07 passes locally.

## 4. Checks

- [ ] 4.1 Run `make`, `make l1`, `make l2` and `make deny`. Verify: all pass, and `scripts/check-test-cases.py` reports TC-145-01 to 06 as implemented.

## Workflow follow-up

- After the merge, archive this change. The sync renames and modifies the requirement of `document-search`, and adds one requirement to `demo-frontend`. The main spec of `demo-frontend` comes with #150, so archive after #150 merges.
- Close #145 when the archive merges.
