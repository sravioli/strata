# Tasks

Each task that adds the test of a TC-9 scenario also deletes the `- **PENDING**` line of that scenario in `specs/proof-verification/spec.md`. Thus `scripts/check-test-cases.py` passes after each task. Each TC test carries the literal ID, for example `TC-9-06`, in its docstring, because the check finds only that form. A test name such as `test_tc_9_06` alone is not enough.

## 1. Pinned toolchain

- [x] 1.1 Add `toolchain.env` at the repository root. It holds each pin in one place: the Aeneas commit `0d21a9ec7bbbb110110cacc14464a47fd71c8705`, the release `nightly-2026.11.14-0d21a9e`, the package `aeneas-linux-x86_64.tar.gz` with SHA-256 `1dffe7bcb8bcb58e40a1607b4d52232d02fe4049f827d595e022d5729835cb6a`, the Charon commit `454586e9b770231e66d2062993e632dfc06463e2`, and Rust `nightly-2026-09-17`. The Lean version stays in `lean/lean-toolchain` (task 2.1). Verify: `bash -n` and shellcheck pass on each script that sources the file.
- [x] 1.2 Add `scripts/install-aeneas.sh DEST`. It downloads the package into a new empty folder and runs `sha256sum -c` against the pin before it extracts the package. The sources, in this sequence:
  1. The release of Aeneas.
  2. The asset with the same name in the Strata release `aeneas-nightly-2026.11.14-0d21a9e`.
  3. A build of the pinned commit from source with opam 5.3.0 and dune, as in the README of the spike.

  A SHA-256 that does not agree stops the script with exit code 1. In that case, the script never goes on to the next source. A source that is not available (HTTP error) goes on to the next source. Find out if the package contains Charon. If not, the script also builds or downloads Charon at its pinned commit, and records how in a comment. Verify: the script installs `aeneas` and `charon`, and `charon version` prints 0.1.315.
- [x] 1.3 Add `scripts/tests/test_install_aeneas.py`. It runs the script with a local fixture server or `file://` URLs, and covers these cases:
  - A package whose SHA-256 is wrong: the script exits 1, and the folder holds no extracted file. Name the test `test_tc_9_06_wrong_sha_stops`.
  - First source not available: the script uses the second source.

  Remove the PENDING line of TC-9-06. Verify: the tests pass, and each test fails when its rule is removed from the script.
- [x] 1.4 Owner step: put a copy of `aeneas-linux-x86_64.tar.gz` into the Strata release `aeneas-nightly-2026.11.14-0d21a9e`. Give the owner the exact `gh release create` command and the SHA-256 to compare. The owner runs it. Verify: `gh release view aeneas-nightly-2026.11.14-0d21a9e` lists the asset, and its digest is the pinned SHA-256.

## 2. Lean project

- [x] 2.1 Add `lean/` with this layout:
  - `lean-toolchain`: `leanprover/lean4:v4.36.0-rc2`.
  - `lakefile.toml`: it requires the Aeneas Lean library at the pinned commit (`subDir = "backends/lean"`), as in the spike.
  - `Strata.lean`: it imports all modules.
  - `Strata/Model/`: the models, written by hand.
  - `Strata/Generated/`: the output of Aeneas. Do not edit these files.
  - `Strata/Proofs/`: the refinement theorems and the theorems of the tables.

  Add `lean/.lake/` to `.gitignore`, and commit `lake-manifest.json`. Verify: `lake build` passes with an empty model.
- [x] 2.2 Measure the first build in CI. Compare three ways to get the build state of Mathlib and of the Aeneas library: `lake exe cache get`, the release asset `lean-build-aeneas-x86_64-unknown-linux-gnu.tar.gz`, and `actions/cache` of `lean/.lake/packages`. The repository cache limit is 10 GB. Use the fastest way that does not build Mathlib. Record the times in a comment in the workflow. Verify: a second run of the job does not compile Mathlib, as the log shows.

## 3. Verification scripts

- [x] 3.1 Add `scripts/proof-obligations.py`. It reads each `docs/tech-design/*.md` that has the line `- Status: accepted` and a `## Proof obligations` table. For each row, it gives the theorem name (the first column, without backticks) and the Rust functions (the last column). It also gives the design file. A table is enforced when a function that it names exists in the core crate: a line that matches `fn <name>\b` in `crates/core/src/**/*.rs`. The script gives only the rows of enforced tables. It prints JSON. Add `scripts/tests/test_proof_obligations.py` with fixtures:
  - an accepted design.
  - a design that is not accepted.
  - a design whose section is N/A, as in 0009.
  - a row that names two functions, as `scope_agrees` does.
  - an accepted table whose function is not in the core crate: no rows.
  - the same table after the function exists: all rows.

  Verify: on the repository, the script gives the 9 theorems of 0004 and the 6 theorems of 0005, and the functions `authorize`, `authorized_scope` and `job_step`.
- [x] 3.2 Add `scripts/check-proofs.py`. It writes a temporary Lean file. The file imports `Strata`, and has `#print axioms Strata.<name>` for each theorem of task 3.1 and for each `Strata.<function>_refines`. It runs the file with `lake env lean` in `lean/` and reads the output:
  - an unknown name fails, and the message names the theorem and the design, or the function.
  - an axiom that is not `propext`, `Classical.choice` or `Quot.sound` fails, and the message names the theorem and the axiom.
  - `sorryAx` fails as an unfinished proof.

  It also searches each `.lean` file under `lean/Strata/` for the words `sorry` and `admit`, and fails with the file and the line. Verify: the script passes on the project when the models and proofs are complete (groups 4 and 5).
- [x] 3.3 Add `scripts/tests/test_check_proofs.py`. Each test builds its own small Lean project in a temporary folder. These use core Lean only, with no Mathlib, so that each test runs in seconds. The tests:
  - `test_tc_9_02_sorry_fails`: a theorem with `sorry`.
  - `test_tc_9_03_aeneas_axiom_fails`: a theorem that uses an `axiom` that Aeneas could generate. The message names the theorem and the axiom.
  - `test_tc_9_04_missing_theorem_fails`: a fixture table names a theorem that is not in the project. The message names the theorem.
  - A function with no `_refines` theorem fails.
  - A complete project passes.

  Remove the PENDING lines of TC-9-02, TC-9-03 and TC-9-04. Verify: the tests pass in the `verify` job.
- [x] 3.4 Add `scripts/translate.sh [--check]`. It runs `charon cargo --preset=aeneas` in `crates/core`, with the pinned nightly and one `--start-from` for each function of task 3.1, and runs `aeneas -backend lean` into `lean/Strata/Generated/`. With `--check`, it writes into a temporary folder, runs `diff -r` against `lean/Strata/Generated/`, and fails with the names of the files that differ. Verify: a second run gives no difference, as spike #8 found.
- [x] 3.5 Add `scripts/tests/test_translate.py` with `test_tc_9_01_stale_translation_fails`. The test copies the core crate to a temporary folder, changes one proved function, and runs `translate.sh --check` against the generated code of the repository. The check must fail and name the file. Remove the PENDING line of TC-9-01. Verify: the test passes in the `verify` job.
- [x] 3.6 Add `scripts/check-toolchain.sh`. It fails when a `rust-toolchain` or `rust-toolchain.toml` file exists outside `scripts/tests/fixtures/`, and the message names the file. The test builds a crate with `#![feature(never_type)]` in a temporary folder, outside the workspace. Add `scripts/tests/test_toolchain.py`:
  - `test_tc_9_05_nightly_feature_fails_on_stable`: `cargo +stable build --manifest-path` on the fixture fails.
  - A temporary `rust-toolchain` file makes `check-toolchain.sh` fail.

  Remove the PENDING line of TC-9-05. Verify: the tests pass, and the `rust` job runs `check-toolchain.sh`.

## 4. Authorization: `authorize` and `authorized_scope`

- [x] 4.1 Write the Lean model in `lean/Strata/Model/Authorization.lean`:
  - `Principal`: an account (tenant, account, account status, tenant status), or a job (tenant, tenant status).
  - `Action` with `forAccount`, and `Resource` with `tenant`.
  - `Scope` (`nothing` or `tenant t`) with `contains`.
  - `authorize` and `authorizedScope`.

  Also write the model statements of the 9 rows of 0004. The Lean statements in the table are about this model. Verify: `lake build` passes, and a proof on the model exists for each model statement.
- [x] 4.2 Write `crates/core/src/authorization.rs` in the proved subset:
  - The types match the model.
  - `authorize` returns `Decision`, which is `Allow` or `Deny(DenyReason)`. The three reasons of 0004 are: not active, action not permitted, different tenant.
  - `authorized_scope` returns `Scope`. Its constructor is private to the crate, so that only `authorized_scope` creates a scope.
  - The tenant and account identifiers are `u64`.

  Add L0 tests for the deny reasons. The reasons are not in a theorem. Verify: `cargo test` and clippy pass.
- [x] 4.3 Run `scripts/translate.sh`. Commit `lean/Strata/Generated/`. In `lean/Strata/Proofs/Authorization.lean`, prove these:
  - `Strata.authorize_refines` and `Strata.authorized_scope_refines`: no panic, and the result maps to the result of the model.
  - `Strata.authorize_total`: the conjunction of the two refinement theorems.
  - The 8 other theorems of the 0004 table, each with its table name in the namespace `Strata`. Each theorem states its property on the Rust functions through the abstraction functions.

  Verify: `scripts/check-proofs.py` passes for the 9 names of 0004 and for the two refinement theorems.

## 5. Job state: `job_step`

- [x] 5.1 Write the Lean model in `lean/Strata/Model/Job.lean`:
  - `Job` with these fields: `state`, `attempts`, `runAfter`, `leaseUntil`, `endedAt` and `lastError`.
  - `Event`: `claim (tenantActive : Bool)`, `succeed`, `fail`, `leaseExpired` and `requeue`.
  - `ErrorClass`, at least `tenantNotActive` and `leaseExpired`.
  - `jobStep j e now`, which follows "Life of a job" in 0005.
  - `maxAttempts`, a constant that matches `MAX_ATTEMPTS`. `requeue` sets `attempts` to zero (TC-5-07).

  Also write the model statements of the 6 rows of 0005, where `max` is `maxAttempts`. Verify: `lake build` passes, and a proof on the model exists for each model statement.
- [x] 5.2 Write `crates/core/src/job.rs` in the proved subset:
  - `job_step(job, event, now)` uses saturating additions for times, the constants `MAX_ATTEMPTS` and `LEASE_SECONDS`, and `backoff_seconds(n)` for the backoff schedule of 0005.
  - An event that does not apply leaves the job unchanged.

  Add L0 tests for the backoff and the lease times. Verify: `cargo test` and clippy pass.
- [x] 5.3 Translate again, and prove these:
  - `Strata.job_step_refines`.
  - The 6 theorems of the 0005 table, with their table names in the namespace `Strata`.

  If the saturating addition gives a Lean axiom, rewrite it in the proved subset (`if a > MAX - b`), so that check 3 passes. Verify: `scripts/check-proofs.py` passes for all 15 names and the three refinement theorems.

## 6. CI

- [x] 6.1 In `.github/workflows/ci.yml`, replace the `lean` filter and job with a `proofs` filter and a `verify` job.
  - The filter: `crates/core/**`, `lean/**`, `scripts/{translate,install-aeneas,check-*,proof-obligations}*`, `scripts/tests/**`, `docs/tech-design/**`, `toolchain.env` and `Cargo.lock`.
  - The job, in sequence:
    1. Install the tools with `install-aeneas.sh`, and the pinned nightly with `rustc-dev`.
    2. `translate.sh --check`.
    3. Get the Lean build state (task 2.2), then `lake build`.
    4. `check-proofs.py`.
    5. The unit tests of the checks.

  Add `verify` to `needs` of `ci-ok`. Run the `rust` job also on `check-toolchain.sh`. Verify: actionlint and zizmor pass, and `verify` is green on the pull request.
- [x] 6.2 Show each check failing in CI once. On a branch that is not merged, push four commits:
  1. a stale translation.
  2. a `sorry`.
  3. an unknown name in a table.
  4. a nightly feature.

  Record the four red job links in the pull request. Then remove the branch. Verify: each run fails at the expected step.

## 7. Documentation and pull request

- [x] 7.1 Update `docs/agents/testing.md`. Add how to install the tools, how to translate, how to build the Lean project, and how to run the checks locally. Add the rule that a change to a proved function includes the new translation and the updated proofs. Verify: markdownlint and ste-lint pass, and each command runs as written.
- [ ] 7.2 Run `python3 scripts/check-test-cases.py`. No PENDING line of a TC-9 scenario stays. Run the `code-review` skill. Open the pull request with the `pr` skill. Its body says `Closes #11`. Verify: all CI jobs and `ci-ok` are green.

## Workflow follow-up

- The owner merges the pull request. This closes #11.
- Archive this change with `openspec-archive-change`.
