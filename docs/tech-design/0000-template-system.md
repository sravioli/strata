<!--
Tech design template for the SYSTEM: how Strata is built and run. Storage,
queues, sandboxing, deployment, capacity. For what Strata does, use
0000-template-rfc.md.

Adapted from the OpenBMC design template (github.com/openbmc/docs,
designs/design-template.md, CC BY 4.0). Changed for Strata: header fields,
Impacts split into three parts, Testing replaced by Test cases, Unresolved
questions added.

Copy to NNNN-slug.md, where NNNN is the issue number. Write "N/A" with a
reason in any section that does not apply.
-->

- Issue: #0000
- Status: draft | accepted | rejected
- Start date: YYYY-MM-DD
- Decided by: (owner's name and date, filled in on the decision)

# {Title}

## Problem description

(1 paragraph) What are we doing and why? What are the goals and the non-goals? Understandable to someone unfamiliar with the project.

## Background and references

(1-2 paragraphs) The context needed, with links to related work and sources. Background only: no design and no requirements here.

## Requirements

(2-5 paragraphs) The constraints. Who uses this? What must be produced? Estimate the relevant quantities: how big is the data, what are the request rates, how does it relate to the capacity ceiling?

## Proposed design

(2-5 paragraphs) A short overview of the design. Major structural elements, technologies chosen, new components. Use a diagram when it helps. No code.

## Alternatives considered

(2 paragraphs) The designs leaned away from, why each was considered and why it was rejected.

## Impacts

### Infrastructure

New or changed services, storage, queues, network paths, secrets, cost, and what must be observable (metrics, logs, alerts). How it is rolled out and rolled back.

### Security and tenant isolation

How the tenant boundary is preserved, what a compromised component could reach, and what new input is untrusted.

### Data and migration

Schema changes, backfills, and how old and new versions of the code run side by side.

### Developer

What changes for someone working on the code: local setup, build time, new tools.

## Test cases

Levels and rules are in `docs/agents/testing.md`. Say in one line what the L0 tests cover. List every L2 and L3 test case, and every L1 test case the owner has to decide.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-0000-01 | | | | pending |

## Unresolved questions

- What must the owner answer before this design can be accepted?
- What is expected to be resolved during implementation?
