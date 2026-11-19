---
status: "accepted"
date: 2026-11-19
decision-makers: sravioli
tech-design: docs/tech-design/0009-verification-boundary.md
---

# Lean proves a pure core of translated Rust, and CI checks each proof

## Context and Problem Statement

Strata uses Lean to prove that parts of its Rust code are correct. A proof is useful only if it is about the code that runs. Also, the boundary between proved code and tested code must be clear. Spike [#8](https://github.com/sravioli/strata/issues/8) showed that Charon and Aeneas can translate a pure and simple Rust core into Lean. What does Lean prove, and what does "proved" mean? The issue is [#9](https://github.com/sravioli/strata/issues/9).

## Considered Options

* Prove the translated Rust for a pure core, with a Lean model and differential tests as the fallback
* Prove all of the code
* Only a Lean model and differential tests for all code
* Pin the nightly of Charon for the whole workspace
* Build Aeneas from source in CI
* Import the code of the spike into the workspace

## Decision Outcome

Chosen option: "Prove the translated Rust for a pure core, with a Lean model and differential tests as the fallback". A proof on the translated code is about the code that runs, and the spike showed that it finds bugs that tests do not find.

The decision has these parts:

* Route 1, prove the Rust: pure functions in the proved subset of Rust. These are `authorize`, `authorized_scope`, `job_step` and the state machine of one revision.
* Route 2, a Lean model and differential tests: logic that has a model but is outside the proved subset. An example is a document with several revisions.
* Route 3, a Lean specification and a specialised path: the search path at M2 tries this route first.
* The proved subset: enums, structs, `match`, `Option`, `Result` with `?`, integers with no overflow, static traits and generics, and simple loops. No `&dyn Trait`, iterator chains, strings or collections.
* A theorem is proved only when CI passes all of these checks:
  * A fresh translation agrees with the Lean code in the repository.
  * No `sorry` and no `admit`.
  * The theorem depends only on `propext`, `Classical.choice` and `Quot.sound`.
  * Each function of the proved core has a refinement theorem.
  * Each theorem that an accepted design names exists.
* The "Proof obligations" tables of the accepted designs are the only list of theorems.
* The Lean model and the theorem statements come first. The Rust code follows them.
* The crates build on stable Rust. Only the translation uses the nightly of Charon, because Charon needs the internal libraries of the compiler.
* CI installs the prebuilt release of Aeneas for the pinned commit, and checks its SHA-256. A copy goes into a Strata release. A build from source is the fallback.
* The verification job runs on each pull request that changes core or Lean code.
* The workspace does not import the code of the spike. The branch of the spike stays as the record.

### Consequences

* Good, because each proof is about the current Rust code, and CI rejects a proof that depends on an assumption.
* Good, because the boundary between proved code and tested code is written down.
* Good, because the crates do not depend on a nightly compiler.
* Bad, because Charon and Aeneas are not verified. A fault in them can make a proof about code that differs from the Rust code. Tests stay where code touches the tenant boundary.
* Bad, because four pins move together, and an upgrade can break proofs.
* Bad, because the proved subset excludes collections, so some core logic is only tested.
* Not decided: the layout of the Lean project, and the cache of Mathlib in CI. Also route 3 for search (M2), and the revision lifecycle with several revisions.
