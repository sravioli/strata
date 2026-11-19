Issue: #11

# Proposal

## Why

Strata has no Rust code yet. The verification work of tech design 0009 needs a Cargo workspace, a core crate and CI jobs that run each test level. This change is the first of two changes for issue #11. It adds the parts that do not depend on Lean. The second change then adds only the checks of 0009, the Lean project and the proved functions.

## What Changes

- Add a Cargo workspace at the repository root, and one crate `strata-core` in `crates/core` with no functions. The crate builds on stable Rust. The second change adds `authorize`, `authorized_scope` and `job_step` to this crate.
- Add `rust` with build-mode `none` to the CodeQL matrix. CodeQL fails on a language with no source files, so this item and the first crate go into one pull request.
- Add `compose.yaml` that starts PostgreSQL and Garage for development and for L1 tests. Garage starts as a cluster of one node, with one bucket and one access key.
- Add the test case check: a script that compares the test case IDs in tech designs and OpenSpec specs with the IDs that tests carry. CI runs it.
- Add an L1 job to CI. The job runs on each pull request, starts the services of `compose.yaml` and runs the L1 tests. The present `rust` job stays the L0 job.
- Add a rule for L1 tests: an L1 test target has the Cargo feature `l1`, so that the L0 job does not run it.

Not in this change: the checks of 0009, the Lean project, the functions of the core crate and their theorems. The second change of issue #11 adds them.

## Capabilities

### New Capabilities

- `test-case-check`: the rules that make sure that each test case ID has an automated test that carries the ID, or a `pending` marker.
- `dev-services`: the local services (PostgreSQL and Garage) that a developer and the L1 job of CI start from one compose file.

### Modified Capabilities

None.

## Impact

- New files: `Cargo.toml`, `Cargo.lock`, `crates/core/`, `compose.yaml`, `dev/garage.toml`, `scripts/check-test-cases.py` and its tests.
- Changed files: `deny.toml` (private crates have no licence), `.github/workflows/ci.yml` (the L1 job and the test case check), `.github/workflows/codeql.yml` (the `rust` language), `docs/agents/testing.md` (the `l1` feature rule and the place of the `pending` marker).
- The `deny` job of CI starts to run, because a `Cargo.toml` exists.
- "L0 on every push" (`docs/agents/testing.md`) means: each push to `main`, and each push to the branch of a pull request. CI already runs for these events. A push to a branch with no pull request runs no CI. This change keeps that. The owner must accept this reading.
- Developers need Docker with Compose v2 to run L1 tests.
- Garage is AGPLv3. `deny.toml` already permits this licence (ADR 0005). The compose file uses the image and links no Garage code.
