# Decision workflow

Every change to what Strata does, or to how it is built, passes through a **tech design** that the owner decides on. Until the owner decides, everything is a **draft**: recommendations written in issues, the ADRs in `docs/adr/draft/`, and any tech design whose status is not `accepted`.

## Steps

1. **Issue.** Start from a GitHub issue (see `issue-tracker.md`). If none covers the work, create one under the map first. Done when you have its number.

2. **Tech design.** On a branch, write `docs/tech-design/NNNN-slug.md`, where `NNNN` is the issue number, from one of the two templates in that folder:
   - `0000-template-rfc.md` for behaviour: what Strata does. Domain logic, APIs, algorithms, anything with theorems.
   - `0000-template-system.md` for the system: how Strata is built and run. Storage, queues, sandboxing, deployment, capacity.

   Open a draft pull request titled `design: <issue title>` whose body says `Refs #<issue>`. Done when the pull request is open, every section is filled or marked N/A with a reason, and every question the issue asks is either answered or listed under Unresolved questions.

3. **Owner decides.** Hand the pull request to the owner and wait. The owner accepts, edits or rejects the design, and with it the test cases and theorem statements that need the owner (see `testing.md`). Done when the owner has said which, in the pull request or in the session.

4. **ADR.** In the same pull request, set the design's status, and record an accepted decision as `docs/adr/NNNN-slug.md` from `docs/adr/0000-template.md`, numbered in sequence. Add any terms it settles to `GLOSSARY.md`. Done when the owner has merged the pull request.

5. **Implement.** Create an OpenSpec change with `/openspec-propose`. Its `design.md` is one line pointing at the tech design; its specs carry the test cases as scenarios; its `tasks.md` is the plan. Work through it with `/openspec-apply-change` in a separate pull request, then archive the change. Done when that pull request is merged and the issue is closed.

The pull request that finishes an issue's work says `Closes #<issue>`. An issue that is only a decision has nothing to implement, so it skips step 5 and its design pull request closes it.

## Writing

Write every tech design, ADR and OpenSpec artifact with the `asd-ste100` skill. The aim is simple, clear text that keeps every detail.

## Research and prototype issues

Issues labelled `wayfinder:research` or `wayfinder:prototype` answer a question for a later tech design, so they skip steps 2 to 5. Post the findings as a comment on the issue and link the branch. The branch stays unmerged; code from it reaches `main` only through a tech design.

## The owner and Lean

Agents write the Lean. The owner reads the plain-English statement of each theorem in the design's Proof obligations table and approves that; CI checks the proof. A theorem whose statement the owner has not approved is a draft.

## Landing work

`main` is protected: every change arrives through a pull request, and the owner merges. Commit messages follow Conventional Commits (`cog.toml`).
