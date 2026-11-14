# Decision workflow

Every change to what Strata does, or to how it is built, passes through a **tech design** that the owner decides on. Until the owner decides, everything is a **draft**: recommendations written in issues, the ADRs that came with the handoff package, and OpenSpec changes.

## Steps

1. **Issue.** Start from a GitHub issue (see `issue-tracker.md`). If none covers the work, create one under the map first. Done when you have its number.

2. **Tech design.** On a branch, create an OpenSpec change for the issue with `/openspec-propose`. Its `design.md` is the tech design. Open a draft pull request titled `design: <issue title>` whose body says `Refs #<issue>`. Done when the pull request is open and every question the issue asks is either answered in `design.md` or listed there as open.

3. **Owner decides.** Hand the pull request to the owner and wait. The owner accepts, edits or rejects the design. Done when the owner has said which, in the pull request or in the session.

4. **ADR.** In the same pull request, record the accepted decision as `docs/adr/NNNN-slug.md` with the owner's name and the date, and add any terms it settles to `GLOSSARY.md`. Done when the owner has merged the pull request.

5. **Implement.** Work through the change's `tasks.md` with `/openspec-apply-change` in a separate pull request, then archive the change. Done when that pull request is merged and the issue is closed.

The pull request that finishes an issue's work says `Closes #<issue>`. For an issue that is only a decision, that is the design pull request.

## Research and prototype issues

Issues labelled `wayfinder:research` or `wayfinder:prototype` answer a question for a later tech design, so they skip steps 2 to 5. Post the findings as a comment on the issue and link the branch. The branch stays unmerged; code from it reaches `main` only through a tech design.

## The owner and Lean

Agents write the Lean. The owner reads the plain-English statement of each theorem and approves that; CI checks the proof. A theorem whose statement the owner has not approved is a draft.

## Landing work

`main` is protected: every change arrives through a pull request, and the owner merges. Commit messages follow Conventional Commits (`cog.toml`).
