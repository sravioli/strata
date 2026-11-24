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

- **Status after the spec exists.** When a test case has its spec scenario, the spec is its only status. The row in the tech design does not get the status. A change to the text of a row follows "Changes to an accepted design" in `decision-workflow.md`.
- **Pending marker.** A scenario with no automated test yet has the line `- **PENDING** <reason>` in its body.
- **Check.** `python3 scripts/check-test-cases.py` enforces these rules. CI runs it on each run.
- **Report.** `python3 scripts/check-test-cases.py --report` prints one line for each ID: its state, its design, its spec and its tests. The states are `approved` (a design row only) and `planned` (a spec scenario with the pending marker). They are also `implemented` (a test carries the ID) and `failing` (the check fails on the ID).

## Running the checks

The `Makefile` holds the commands of CI. Each CI job of `.github/workflows/ci.yml` runs one target, so a local run and a CI run use the same commands.

- **First run:** `make setup`. It installs the pinned tools into `.tools/` and checks for the toolchains. Run it again after a pin changes.
- **Before each commit:** run `make`. It runs `lint`, `prose`, `test-cases` and `rust`.
- **Other targets:** `deny`, `verify`, `sql`, `l1`, `commits` and `secrets`. Each has the name of its CI job.
- **Pinned tools.** Each target installs the pinned tools that it needs into `.tools/`, so CI and a local run use the same versions. A download whose SHA-256 is not the pin stops the run.
  - Lint and SQL tools: `.github/requirements/tools.txt` (uv) and `package-lock.json` (npm). `make tools` installs them.
  - `cargo-llvm-cov`, `cargo-deny`, `cog` and `betterleaks`: `.github/requirements/tools.env` (URL and SHA-256), with `scripts/install-tool.sh`.
  - Aeneas and Charon: `toolchain.env`, with `scripts/install-aeneas.sh`.
  - nsjail: `.github/requirements/tools.env` (tag and commit), with `scripts/install-nsjail.sh`. nsjail has no release binaries, so the script builds it from source. A tag whose commit is not the pin stops the build. The build needs `g++`, `make`, `pkg-config`, `bison`, `flex`, and the `protobuf` and `libnl-route-3` headers.
- **Toolchains.** Install these yourself: `uv`, `npm`, the Rust toolchain (`rustup`, with the `llvm-tools-preview` component), and for `make verify` Lean (`elan`) and the nightly `CHARON_RUST` of `toolchain.env`. `make rust` and `make l1` need `pdftotext` (`poppler-utils`). `make l1` also needs Docker or Podman, with `psql`, the aws CLI and `python3` for `scripts/dev-services-smoke.sh`, and `make sql` needs `psql` and an empty Postgres. `make setup` warns about each one that is missing.
- **Linter configuration.** The configuration files are in `.github/linters/`. The targets give the path to each tool, so run a linter through `make`, not directly.

## Running the levels

- **L0:** `cargo test --workspace`. CI runs it on each push to `main` and on each pull request.
- **L1:** `docker compose up --wait` (Podman works too: `make l1` uses `podman compose` when `docker` is not installed, and `COMPOSE` overrides the command), then `cargo test --workspace --all-features`. An L1 test target has `required-features = ["l1"]` in its `Cargo.toml`, so L0 does not run it. An L1 test fails when a service is not available. It does not skip. `scripts/dev-services-smoke.sh` tests the services themselves. CI runs L1 on each pull request, with the L0 tests, under `cargo llvm-cov` with the coverage floor of `CONSTRAINTS.md`.

## Repo tooling

Product code is Rust, and the levels above test it. Repo tooling (the checks and helpers in `scripts/`) is Python 3 with the standard library only. Its tests are in `scripts/tests/`: run `python3 -m unittest discover -s scripts/tests`. Tooling is not a crate, so the workspace holds only product code. Move tooling to a Rust `xtask` crate only when it must share types with the product or parse Rust code.

## Proofs

Tech design 0009 sets when a theorem is "proved". `make verify` runs its checks (steps 2 to 5 below, in check mode). Run them by hand in this sequence:

1. Install the pinned tools: `make setup` puts Aeneas and Charon in `.tools/aeneas/`. Outside `make`, add that folder to `PATH`. Charon also needs the nightly of `toolchain.env` with the components `rustc-dev`, `llvm-tools` and `rust-src`.
2. Translate the core crate: `scripts/translate.sh`. It writes `lean/Strata/Generated/`. `scripts/translate.sh --check` only compares.
3. Build the Lean project: `cd lean && lake exe cache get && lake build`.
4. Check the theorems: `python3 scripts/check-proofs.py`.
5. Run the tests of the checks: `python3 -m unittest discover -s scripts/tests`.

Rules:

- A change to a proved Rust function includes the new translation and the updated proofs in the same pull request.
- A "Proof obligations" table is enforced when a Rust function that it names exists in `crates/core/src`. From then on, each theorem of the table must exist as `Strata.<name>`, and each function must have `Strata.<function>_refines`.
- The crates build on stable Rust. No crate has a `rust-toolchain` file. `scripts/check-toolchain.sh` checks this.
- An upgrade of the pins in `toolchain.env`, `lean/lean-toolchain` and `lean/lakefile.toml` is one pull request that moves all of them together.
