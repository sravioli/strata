#!/usr/bin/env sh
# Makes the skills in .agents/skills available to each coding agent.
#
#   Codex and GitHub Copilot read .agents/skills directly: nothing to do.
#   Claude Code reads .claude/skills: this script links each skill there.
#
# Run it after adding a skill, or after a clone on a system where git did
# not check the links out as links (Windows without symlink support).
#
# To add a skill:
#   npx skills add <owner>/<repo> -a claude-code codex github-copilot
set -eu
cd "$(dirname "$0")/.."
mkdir -p .claude/skills
for dir in .agents/skills/*/; do
  name=$(basename "$dir")
  ln -sfn "../../.agents/skills/$name" ".claude/skills/$name"
  echo "claude: linked $name"
done
echo "codex, copilot: read .agents/skills directly"
