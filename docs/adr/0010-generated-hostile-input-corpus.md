---
status: "accepted"
date: 2026-11-21
decision-makers: sravioli
tech-design: docs/tech-design/0048-hostile-input-corpus.md
---

# A script generates the hostile files, and an L1 check runs them in the real sandbox

## Context and Problem Statement

ADR 0006 (extraction sandbox) puts each parse in a sandbox with hard limits, and gives a class to each failure. How does Strata show that each limit stops a hostile file, and that no parser crashes on one? The issue is [#48](https://github.com/sravioli/strata/issues/48).

## Considered Options

* Generated files, and an L1 check with a manifest
* A separate kind of test, next to L0 to L3
* Hostile files in git
* Real malformed files from a public set
* Fuzzing

## Decision Outcome

Chosen option: "Generated files, and an L1 check with a manifest". Generated files reach each limit, have no licence question and stay out of git. The check runs against real local dependencies, nsjail and the kernel, so it is an L1 test.

The decision has these parts:

* A manifest in git gives each entry a recipe, the expected result and the limit or check that must stop it. The expected result is one class, a set of permitted classes, or text.
* A Python script with the standard library only builds each file again on each run, with the same bytes each time.
* The check runs the real `strata-extract` in the real sandbox, through the code of the worker. It runs each PDF with both parsers.
* `ExtractionCrash` is never an expected result. A crash stops the merge.
* CI runs the check on each pull request and each push to `main` that changes extraction, with a path filter. The required check `ci-ok` needs the job.
* The owner decides the categories of entries in the tech design. A new entry in a category needs no decision.
* Format detection reads the start of the file.
* A file that needs a password gets the new class `EncryptedFile` (ADR 0006). Strata does not decrypt files in v1 ([#98](https://github.com/sravioli/strata/issues/98)).

### Consequences

* Good, because each change to extraction runs each limit before the merge.
* Good, because git holds no hostile file, except small regression files from fuzzing.
* Bad, because generated files rarely find a crash inside PDFium or Poppler. Real files ([#94](https://github.com/sravioli/strata/issues/94)) and fuzzing ([#93](https://github.com/sravioli/strata/issues/93)) are later decisions.
* Bad, because the check needs Linux with user namespaces and nsjail, also on the CI runner.
* Bad, because the check takes minutes, because some files run to the wall-clock limit.
