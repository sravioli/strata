# Testing

## Test levels

| Level | What it runs against | Who decides the scenarios | When it runs |
|---|---|---|---|
| **L0** | Nothing outside the process: pure functions, in memory | The agent | Every push |
| **L1** | Real local dependencies in containers: PostgreSQL, object storage | The agent proposes; the owner decides any scenario that fixes a product behaviour | Every pull request |
| **L2** | The whole system, end to end, in a deployed environment | The owner | Before a release |
| **L3** | The whole system under load, measured against a target number | The owner, who also sets the number | Before a release, and on demand |

A property proved in Lean needs no L0 test of the same property. It does need a differential test wherever the Rust code is not the code the proof is about.

## Test cases

A **test case** is a scenario written in plain words, with an automated test that executes it. The words come first and are what the owner decides on.

- **ID.** `TC-<issue>-<n>`, for example `TC-3-02`. An ID is never reused or renumbered.
- **Text.** Given, when, then. An L3 test case states its target number.
- **Where it lives.** It is born as a row in the tech design. During implementation it becomes a scenario in the OpenSpec spec, with the ID in the scenario's name; that is its permanent home.
- **Automated test.** Its name or doc comment carries the ID, so a search for the ID finds the text and the test.

Every L2 and L3 test, and every L1 test the owner decided, is a test case. Other L0 and L1 tests are ordinary tests and need no ID.

A test case with no automated test yet is listed as `pending`; a test case is never silently dropped.

- **Status after the spec exists.** When a test case has its spec scenario, the spec is its only status. The row in the tech design is not updated: an accepted design stays as the owner decided it.
- **Pending marker.** A scenario with no automated test yet has the line `- **PENDING** <reason>` after its steps.
- **Check.** `python3 scripts/check-test-cases.py` enforces these rules. CI runs it on each run.

## Running the levels

- **L0:** `cargo test --workspace`. CI runs it on each push to `main` and on each pull request.
- **L1:** `docker compose up --wait`, then `cargo test --workspace --all-features`. An L1 test target has `required-features = ["l1"]` in its `Cargo.toml`, so L0 does not run it. An L1 test fails when a service is not available. It does not skip. `scripts/dev-services-smoke.sh` tests the services themselves. CI runs L1 on each pull request.
