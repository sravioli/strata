# Strata

## Ownership

Claude owns this codebase and maintains it. Agents write the code, the docs and the tests, and keep them healthy. Humans supervise: they review, decide and merge. Elsewhere in these docs, "the owner" means that human supervisor.

## Decision workflow

Tech design first: before implementing anything or settling a decision, write a tech design in `docs/tech-design/` tied to a GitHub issue, and wait for the owner to decide. Everything is a draft until then. Repo chores (skills setup, CI config) are exempt. See `docs/agents/decision-workflow.md`.

## Testing

Test levels L0 to L3 and test cases: read before writing a test or proposing test scenarios. See `docs/agents/testing.md`.

### Issue tracker

Issues live in GitHub Issues for sravioli/strata (via `gh`). See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context: one `GLOSSARY.md` + `docs/adr/` at the repo root. ADRs are not final and act as guidelines for now. See `docs/agents/domain.md`.

## Skills

Skills come from two places: `.agents/skills/<name>/SKILL.md`, and the Claude Code plugins `mattpocock-skills`, `ponytail` and `rust-skills`, declared in `.claude/settings.json`. Claude Code names a plugin skill `<plugin>:<skill>`, for example `mattpocock-skills:tdd`; the tables below use the bare name. Before you start a task, check the table below. When a row matches, invoke the skill and follow it before you act. Several rows can match one task: use each in the stage it names.

The repo docs outrank a skill. A skill never lets you skip the decision workflow: for example, `openspec-propose` belongs to step 5 and starts only after the owner accepts the tech design.

### Decide (issue and tech design)

| Skill | Use when |
|---|---|
| `grilling` | A plan or decision has open branches and you must settle them with the owner before you write the design. |
| `research` | An issue needs facts from primary sources: `wayfinder:research` issues, API or library behaviour. |
| `prototype` | A design question needs a throwaway model or UI: `wayfinder:prototype` issues. |
| `domain-modeling` | You add or change a term in `GLOSSARY.md`, or write or edit an ADR. |
| `codebase-design`, `api-and-interface-design` | You shape a module boundary, a seam or a public interface. |
| `doubt-driven-development` | A decision is high stakes or irreversible: auth, security, migrations, data loss. |
| `source-driven-development` | You check an approach against the official docs before you settle it: Postgres, Lean, Aeneas, any library. |
| `constraint-driven-development` | The repo needs a written bar that applies to every change (coverage floor, no skipped tests, no silenced checks). It goes in `CONSTRAINTS.md`; per-feature scenarios stay in `docs/agents/testing.md`. |
| `asd-ste100` | You write a tech design, ADR or OpenSpec artifact. Required. |

### Implement

| Skill | Use when |
|---|---|
| `openspec-propose` | The owner accepted the tech design and you plan the change. |
| `openspec-apply-change` | You work through the tasks of an OpenSpec change. |
| `openspec-archive-change` | The change is merged and done. |
| `openspec-explore`, `openspec-update-change`, `openspec-sync-specs` | You think through, revise or sync a change that exists. |
| `tdd` | You build a feature or fix a bug test-first. Read `docs/agents/testing.md` first. |
| `ponytail` | You write any code: it keeps the change to the smallest that solves the task. |
| `rust-router` | You write, review or debug Rust: it routes to the ownership, error, concurrency, `unsafe` and domain skills of `rust-skills`. |
| `performance-optimization` | A speed or capacity requirement exists, or you suspect a regression. |
| `deprecation-and-migration` | You remove or replace an old system, API or feature, or move users or data to a new one. |
| `frontend-ui-engineering` | You build or change a user interface. |
| `dba`, `postgres` | You touch a relational database: schema, query, migration. |
| `security-and-hardening` | Code handles untrusted input, auth, secrets, storage or a new dependency. |
| `observability-and-instrumentation` | Code that runs in production needs logs, metrics, traces or alerts. |
| `ci-cd-and-automation` | You change a pipeline or a quality gate. |

### Fix and clean up

| Skill | Use when |
|---|---|
| `diagnosing-bugs` | Something is broken, throwing, failing or slow. |
| `code-simplification` | Working code is harder to read than it should be. |

### Review and land

| Skill | Use when |
|---|---|
| `code-review` | Before you open a pull request, or when asked to review a branch. |
| `shipping-and-launch` | You prepare a production deploy. |
| `ponytail-review`, `ponytail-audit` | You want a plain-English quality review of a change or of the whole repo. |
| `pr` | You write a pull request body. |
| `humanizer` | You write prose for people (README, pull request text) and it reads like AI. |

### Meta

| Skill | Use when |
|---|---|
| `writing-for-agents` | You edit `AGENTS.md`, `CLAUDE.md` or a skill. |
| `context-engineering` | Output quality drops, or you set up context for a new session. |
| `caveman` | The owner asks for short, blunt replies ("caveman mode", "say less"). |
| `wizard` | A step needs a human to act, for example credentials, a third-party dashboard or a cutover. |

### Setup

Skills in `.agents/skills/`: Codex and GitHub Copilot read the folder directly; `scripts/setup-skills.sh` links it into `.claude/skills/` for Claude Code. Add one with `npx skills add <owner>/<repo> -a claude-code codex github-copilot`, which also records its source in `skills-lock.json`.

Plugins: Claude Code installs the declared marketplaces and plugins when a contributor trusts the folder. Add one with `claude plugin marketplace add <source> --scope project` and `claude plugin install <plugin>@<marketplace> --scope project`. Codex and Copilot do not load these plugins, so a skill that every agent needs belongs in `.agents/skills/`.

Add every new skill to the tables above in the same pull request.
