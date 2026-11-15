## Decision workflow

Tech design first: before implementing anything or settling a decision, write a tech design in `docs/tech-design/` tied to a GitHub issue, and wait for the owner to decide. Everything is a draft until then. Repo chores (skills setup, CI config) are exempt. See `docs/agents/decision-workflow.md`.

## Testing

Test levels L0 to L3 and test cases: read before writing a test or proposing test scenarios. See `docs/agents/testing.md`.

## Agent skills

Project skills live in `.agents/skills/`. Codex and GitHub Copilot read that folder directly; `scripts/setup-skills.sh` links it into `.claude/skills/` for Claude Code. Add a skill with `npx skills add <owner>/<repo> -a claude-code codex github-copilot`, which also records its source in `skills-lock.json`.

### Issue tracker

Issues live in GitHub Issues for sravioli/strata (via `gh`). See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context: one `GLOSSARY.md` + `docs/adr/` at the repo root. ADRs are not final and act as guidelines for now. See `docs/agents/domain.md`.
