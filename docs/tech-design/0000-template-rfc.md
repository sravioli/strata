<!--
Tech design template for BEHAVIOUR: what Strata does. Domain logic, APIs,
algorithms, anything with theorems. For how Strata is built and run, use
0000-template-system.md.

Adapted from the Rust RFC template (github.com/rust-lang/rfcs, MIT OR
Apache-2.0). Added for Strata: Proof obligations, Impacts, Test cases.

Copy to NNNN-slug.md, where NNNN is the issue number. Write "N/A" with a
reason in any section that does not apply.
-->

- Issue: #0000
- Status: draft | accepted | rejected
- Start date: YYYY-MM-DD
- Decided by: (owner's name and date, filled in on the decision)

# {Title}

## Summary

One paragraph explanation of the design.

## Motivation

The problem this solves, with the background needed to understand it, and the specific cases where it helps.

## Guide-level explanation

Explain the design as if it were already built and you were teaching it to someone new to Strata, in plain words:

- Introduce new named concepts.
- Explain largely in terms of examples.
- Explain how an account holder, a tenant administrator or a developer should think about it.

## Reference-level explanation

The technical portion. Enough detail that:

- its interaction with the rest of the system is clear;
- it is reasonably clear how it would be implemented;
- corner cases are dissected by example.

Return to the examples from the previous section and show how the detailed design makes them work.

## Proof obligations

What will be proved in Lean. One row per theorem. The owner approves the plain-English statement; CI checks the proof.

| Theorem | Plain-English statement | Lean statement | Rust function it is about |
|---|---|---|---|
| | | | |

State any assumption a proof rests on (for example, that a database transaction is atomic).

## Impacts

### Infrastructure

New or changed services, storage, queues, network paths, secrets, cost, and what must be observable (metrics, logs, alerts). How it is rolled out and rolled back.

### Security and tenant isolation

How the tenant boundary is preserved, what a compromised component could reach, and what new input is untrusted.

### Data and migration

Schema changes, backfills, and how old and new versions of the code run side by side.

## Test cases

Levels and rules are in `docs/agents/testing.md`. Say in one line what the L0 tests cover. List every L2 and L3 test case, and every L1 test case the owner has to decide.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-0000-01 | | | | pending |

## Drawbacks

Why should we *not* do this?

## Rationale and alternatives

- Why is this design the best in the space of possible designs?
- What other designs were considered, and why were they not chosen?
- What is the impact of not doing this?

## Prior art

How other systems solve this, good and bad, and any papers or posts worth reading.

## Unresolved questions

- What must the owner answer before this design can be accepted?
- What is expected to be resolved during implementation?
- What related questions are out of scope here and belong to a later design?

## Future possibilities

Natural extensions of this design. Being listed here is not a reason to accept it.
