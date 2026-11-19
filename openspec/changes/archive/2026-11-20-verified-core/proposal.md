Issue: #11

# Proposal

## Why

Tech design 0009 defines when Strata calls a theorem "proved". CI does not check any of its five rules yet, and the core crate has no functions. This change is the second and last change for issue #11. It adds the checks of 0009, the Lean project and the first proved functions. Then CI proves each theorem in the "Proof obligations" tables of designs 0004 and 0005 on the Rust code that runs.

## What Changes

- Add the Rust functions to `strata-core`, in the proved subset of 0009:
  - `authorize` and `authorized_scope` (tech design 0004).
  - `job_step` (tech design 0005), with saturating additions for times and a constant `MAX_ATTEMPTS`.
- Add a Lean project in `lean/` with three parts:
  - The Lean models of the three functions.
  - The Lean code that Aeneas generates from the core crate.
  - The proofs. A proof on the generated code exists for each theorem that the "Proof obligations" tables of 0004 and 0005 name. It has that exact name, in the namespace `Strata`. Each Rust function also has a refinement theorem `<function>_refines`, and `authorize_total` is the refinement theorem of `authorize` and `authorized_scope`.
- Add the checks of 0009 as scripts. Each script has an exit code and has tests that use fixtures:
  1. Fresh translation: translate the core crate again, and fail on a difference with `lean/` (TC-9-01).
  2. No `sorry` and no `admit` in the Lean code (TC-9-02).
  3. Only the standard axioms for each theorem in an accepted "Proof obligations" table (TC-9-03).
  4. A refinement theorem for each Rust function that a table names.
  5. Each theorem that a table names exists (TC-9-04).
- Add a check that no crate of the workspace pins a nightly toolchain. Add a fixture that shows that stable Rust rejects a nightly feature (TC-9-05).
- Add an install script for Aeneas and Charon. It reads the pinned package, compares its SHA-256 with the pinned value, and stops before it uses a package that does not agree (TC-9-06). The script tries three sources in this sequence: the release of Aeneas, the copy in a Strata release, and a build from source.
- Add the `verify` job to CI. It runs on each pull request and each push to `main` that changes the core crate, the Lean project, the proof scripts or a tech design. It replaces the present `lean` job.

Not in this change: the API, the queue, SQL and the L1 test cases of 0004 and 0005. Their test case rows stay `pending`.

## Capabilities

### New Capabilities

- `proof-verification`: the checks that CI runs before Strata calls a theorem "proved", and the pinned toolchain that the checks use.

### Modified Capabilities

None. The behaviour of `authorize`, `authorized_scope` and `job_step` is in the "Proof obligations" tables of 0004 and 0005. Requirement V6 of 0009 permits no second list of theorems, so no spec repeats them.

## Impact

- New code: `crates/core/src/{authorization,job}.rs`, `lean/`, and these scripts: `scripts/translate.sh`, `scripts/install-aeneas.sh`, `scripts/proof-obligations.py`, `scripts/check-proofs.py`, `scripts/check-toolchain.sh`, with their tests and fixtures.
- Changed: `.github/workflows/ci.yml`. The `verify` job replaces the `lean` job, and the path filter for proofs is new. Also `docs/agents/testing.md`: how to run the checks locally.
- Toolchain pins, which move together: Aeneas `0d21a9e` (release `nightly-2026.11.14-0d21a9e`), Charon `454586e`, Rust `nightly-2026-09-17` for the translation only, and Lean `v4.36.0-rc2` with Mathlib.
- Owner step: put a copy of `aeneas-linux-x86_64.tar.gz` into a release of the Strata repository. The install script uses this copy when the upstream release is not available.
- The `verify` job downloads approximately 1 GB for the tools and a large Mathlib cache. This change measures its time.
- This change closes #11.
