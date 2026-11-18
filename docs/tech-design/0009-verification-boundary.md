- Issue: [#9](https://github.com/sravioli/strata/issues/9)
- Status: draft
- Start date: 2026-11-19
- Decided by: (not decided)

# Verification boundary: what Lean proves, what tests cover, and what "proved" means

## Problem description

Strata uses Lean to prove that parts of its Rust code are correct. A proof is useful only if its scope is clear. Which code is it about, and which rules does CI check? Which code is tested and not proved? This design sets the boundary between proved code and tested code. It also sets the checks that a theorem must pass before Strata calls it "proved", and the toolchain that CI uses.

Goals:

- One definition of "proved" that CI enforces.
- A clear list of the code that Lean proves, and of the code that tests cover.
- A workflow in which the Lean definition comes first and the Rust code follows it.
- A pinned toolchain that CI can install quickly.

Non-goals:

- The theorems of each feature. Each tech design lists its own theorems in "Proof obligations".
- The design of the search path at M2. This design only sets the route that M2 tries first.

## Background and references

The owner position on issue [#9](https://github.com/sravioli/strata/issues/9) (2026-11-14) is: use Lean as much as possible, to guide developers and agents to the fastest correct implementation.

Spike [#8](https://github.com/sravioli/strata/issues/8) tested the route in which Charon and Aeneas translate Rust into Lean, and a proof is about the translated code. Its findings:

- The whole slice is proved: the state machine of one revision, with four invariants and a refinement theorem. The refinement theorem says that the Rust function never panics and gives the result of the Lean model.
- The proof found a design bug that the Rust tests did not find.
- Enums, structs, `match`, integers, `Option`, `Result` with `?`, static traits and generics translate. Each integer `+` needs a precondition that it does not overflow.
- `&dyn Trait` and iterator chains fail. Strings and `Vec` translate with generated axioms for library functions that Aeneas cannot model.
- Four pins move together: Aeneas, Charon, the Rust nightly of Charon, and Lean with Mathlib.

The branch [`spike/8-lean-aeneas`](https://github.com/sravioli/strata/tree/spike/8-lean-aeneas) holds the code of the spike and the commands to reproduce it.

Issue [#9](https://github.com/sravioli/strata/issues/9) names three routes:

1. Prove the Rust: Aeneas translates the Rust code, and a proof shows that it meets the Lean model.
2. Model and differential tests: a Lean reference model, and tests that compare the Rust code with it.
3. Lean invariants that justify a faster, specialised path, for example code generated from a Lean specification. The `lean-derive-hargup` entry of the "$12 server" benchmark is an example: https://benchmarks.arjaythedev.com/12-dollar-server/leaderboard/lean-derive-hargup

References:

- Aeneas: https://github.com/AeneasVerif/aeneas
- Charon: https://github.com/AeneasVerif/charon
- Aeneas releases: https://github.com/AeneasVerif/aeneas/releases

## Requirements

| ID | Requirement |
|---|---|
| V1 | Strata calls a theorem "proved" only when it passes all checks in "The definition of proved". CI enforces each check. |
| V2 | A proved Rust function uses only the proved subset of Rust. |
| V3 | For core logic, the Lean model and the theorem statements come first. The owner approves the plain-English statements in the tech design. Then the Rust code follows. |
| V4 | Code outside the proved core has tests. Where a Lean model of that code exists, the tests compare the code with the model. |
| V5 | The versions of Aeneas, Charon, the nightly of Charon, Lean and Mathlib are pinned, and only a pull request changes them. |
| V6 | The "Proof obligations" table of each accepted tech design is the only list of theorems. No second list exists. |
| V7 | All Rust crates build on stable Rust. |

Quantities:

- The spike needed approximately 8 GB of Lean build state, and 1 GB to build Aeneas from source.
- The prebuilt package of Aeneas for x86_64 Linux is approximately 160 MB. It holds Aeneas, Charon and the built Aeneas library for Lean.

## Proposed design

### Three routes, and where each one applies

| Route | Applies to | Example |
|---|---|---|
| 1. Prove the Rust | Pure functions inside the proved subset of Rust | `authorize`, `authorized_scope` (ADR 0002), `job_step` (ADR 0004), the state machine of one revision |
| 2. Model and differential tests | Logic that has a Lean model but is outside the proved subset | The rules of a document with several revisions, which need a collection and a loop |
| 3. Lean specification and a specialised path | A hot path with a workload that is fully known and does not change | The search path at M2 tries this route first |

For the search path, M2 tries route 3 first. Its design must show whether the workload is stable enough for a path that comes from a Lean specification. If it is not, the search path uses route 1 or route 2. A specialised path that is tested and not proved is "tested" (see "The definition of proved").

### The proved core and the tested code

| Proved (route 1) | Tested, not proved |
|---|---|
| The authorization decision and the authorized scope | SQL, and the policies of row-level security |
| The state machine of a job | The claim of a job with `FOR UPDATE SKIP LOCKED`, leases and time |
| The state machine of one revision | Async orchestration in the API and the worker |
| Later: chunk boundaries (M2), if they fit the proved subset | Object storage, signed URLs, the extraction sandbox and the parsers |

A proof about the model says nothing about the code in the right column. L1 tests cover that code (`docs/agents/testing.md`).

Rank fusion is not a candidate. Its properties are almost trivial, so a proof gives little value.

### The proved subset of Rust

A function that a proof is about uses only:

- Enums, structs, `match`, `Option` and `Result` with `?`.
- Integers. Each operation that can overflow has a precondition, or uses a checked or saturating operation.
- Static traits and generics, and local closures.
- Simple loops, each with a loop invariant in its proof.

It does not use `&dyn Trait`, iterator chains, strings or collections. Charon starts the translation from the functions under proof (`charon --start-from`), so code outside the subset stays out of the translation.

Two accepted designs need these rules:

- `authorized_scope` (ADR 0002) returns a small value, for example "the documents of tenant T". It does not return a list of documents.
- `job_step` (ADR 0004) adds times. Each addition has a precondition that it does not overflow, or uses a saturating operation.

### The definition of proved

A theorem is proved when all of these checks pass in CI:

1. Fresh translation. CI translates the current Rust code with the pinned Charon and Aeneas, and compares the result with the Lean code in the repository. A difference fails the check. Thus a proof is never about an old version of the code.
2. No unfinished proof. No `sorry` and no `admit` in the Lean code on `main`.
3. Only standard axioms. CI runs `#print axioms` on each theorem in the "Proof obligations" tables of accepted designs. The permitted axioms are `propext`, `Classical.choice` and `Quot.sound`. A theorem that depends on an axiom that Aeneas generated for a library function fails the check.
4. Refinement. Each function in the proved core has a refinement theorem: the Rust function never panics, and its result is the result of the Lean model.
5. Each theorem exists. Each theorem that the "Proof obligations" table of an accepted design names exists in the Lean code, and passes checks 2 and 3.

Code that does not pass these checks is "tested", also if a Lean model of it exists.

### Toolchain

The Rust crates build on stable Rust. The verification job of CI also installs the pinned nightly of Charon. Charon reads the Rust code through the internal libraries of the Rust compiler (`rustc_private` and the component `rustc-dev`). These libraries exist only in nightly builds, and each version of Charon needs one exact nightly. Thus the translation always needs that nightly. The crates must not use nightly features, so that they also build on stable.

CI installs Aeneas and Charon from the prebuilt release of Aeneas for the pinned commit. The pinned commit `0d21a9ec` has the release `nightly-2026.11.14-0d21a9e`. CI checks the SHA-256 of the package. A copy of the package goes into a release of the Strata repository. The releases of Aeneas are pre-releases, and Aeneas states no promise to keep them. If both copies are not available, CI builds Aeneas from source.

A full build of Mathlib in CI is slow. Implementation checks whether the built Lean library in the release, or the cache of Mathlib, removes that build.

### Workflow for core logic

1. The tech design states each theorem in plain English, and the owner approves the statements.
2. An agent writes the Lean model and the theorem statements.
3. An agent writes the Rust function in the proved subset.
4. Charon and Aeneas translate the Rust function.
5. An agent proves the theorems and the refinement theorem on the translated code.
6. CI runs the checks in "The definition of proved".

### The code of the spike

The workspace (issue [#11](https://github.com/sravioli/strata/issues/11)) does not import the code of the spike. The spike measured whether the route works. Its slice of the revision lifecycle was not designed in a tech design, and the owner did not accept it. The workspace starts the core crate with `authorize`, `authorized_scope` and `job_step`, which accepted designs define.

The branch of the spike stays as the record of the measurement. It also shows the methods that implementation uses again: `charon --start-from`, the layout of a refinement proof, and the pins. When a design for the revision lifecycle is accepted, its code can start from the slice of the spike.

## Alternatives considered

**Prove all of the code.** Lean cannot see SQL, row-level security, async code or the network. A proof about a model of them does not prove the code that runs. Tests cover them better.

**Only a Lean model and differential tests (route 2) for all code.** This needs no translation and no nightly. But a test compares only the inputs that it tries. The spike showed that a proof on the translated code found a bug that the tests did not find. Route 2 stays the fallback for code outside the proved subset.

**Pin the nightly of Charon for the whole workspace.** Then one toolchain builds all crates. But all crates then depend on a nightly that changes with each upgrade of Charon, and dependencies can fail on it. Stable for the crates, and the nightly only for the translation, keeps that risk in one job.

**Build Aeneas from source in CI.** This needs OCaml and approximately 1 GB of build state, and takes longer for each run without a cache. The prebuilt release gives the same binaries. Building from source stays the fallback.

**Import the code of the spike into the workspace.** It is code that works. But no accepted design defines it, and the decision workflow requires an accepted design first.

## Proof obligations

N/A. This design sets the rules for proofs, and has no theorems of its own. Each feature design lists its theorems. The test cases below check that CI enforces the rules.

## Impacts

### Infrastructure

- A verification job in CI: install the pinned tools, translate, compare, build the Lean project, and check the axioms and the theorem list.
- A release in the Strata repository that holds a copy of the pinned package of Aeneas.
- A cache of the Lean build state in CI.

### Security and tenant isolation

- The theorems of the authorization decision (ADR 0002) are a part of the tenant boundary. The checks of this design make sure that they are about the current code.
- Charon and Aeneas are not verified. A fault in them can make a proof about code that differs from the Rust code. The tests at L0 and L1 stay, also for proved code, where the code touches the boundary.
- The package of Aeneas is a binary dependency. CI checks its SHA-256.

### Data and migration

N/A. No data changes.

### Developer

- A developer who changes a proved function must translate it again and update the proofs in the same pull request.
- An upgrade of the pins is one pull request that moves all pins together. The library of Aeneas for Lean can change between versions, so an upgrade can break proofs.
- Local work needs the pinned tools. The release package gives them without a build of OCaml.
- Issue [#11](https://github.com/sravioli/strata/issues/11) changes: the core crate starts with the functions of accepted designs, and the check for `sorry` becomes the checks of this design.

## Test cases

L0: these checks run in CI on each pull request that changes Rust code of the core or Lean code.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-9-01 | L0 | Given a change to a proved Rust function and no new translation. When CI runs. Then the check of the fresh translation fails. | (not decided) | pending |
| TC-9-02 | L0 | Given a theorem that contains `sorry`. When CI runs. Then the verification job fails. | (not decided) | pending |
| TC-9-03 | L0 | Given a theorem in a "Proof obligations" table that depends on an axiom that Aeneas generated. When CI runs. Then the check of the axioms fails and names the theorem and the axiom. | (not decided) | pending |
| TC-9-04 | L0 | Given a theorem that the "Proof obligations" table of an accepted design names, and that does not exist in the Lean code. When CI runs. Then the check fails and names the theorem. | (not decided) | pending |
| TC-9-05 | L0 | Given a Rust crate that uses a nightly feature. When CI builds it on stable. Then the build fails. | (not decided) | pending |
| TC-9-06 | L0 | Given a package of Aeneas with a SHA-256 that is not the pinned one. When CI installs it. Then the job stops before it uses the package. | (not decided) | pending |

## Unresolved questions

The owner answered these questions on 2026-11-19:

1. Route 3 is not excluded from v1. The search path at M2 tries it first.
2. CI uses the prebuilt release of Aeneas, with a check of the SHA-256 and a copy in a Strata release.
3. The workspace does not import the code of the spike.
4. No second spike now. The rules of a document with several revisions use route 2. A second spike happens only if the model shows that a proof of the loop is worth its cost.

The owner must answer these questions before this design can be accepted:

1. Do you accept that the crates build on stable, and that only the translation uses the nightly of Charon? A stable toolchain for the translation is not possible, because Charon cannot run without the nightly (see "Toolchain").
2. Does the verification job run on each pull request that changes core or Lean code, as this design proposes? Or on each push, as L0 tests do?

Implementation resolves these points:

- Whether the built Lean library in the release, or the cache of Mathlib, removes a full build of Mathlib in CI.
- The layout of the Lean project in the workspace.
- The script that reads the "Proof obligations" tables and gives the theorem names to the checks.

These questions belong to later designs:

- The search path and route 3 (M2).
- The design of the revision lifecycle with several revisions.
- Chunk boundaries (M2).
