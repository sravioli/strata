# The checks of CI. Each target is one job of .github/workflows/ci.yml, and CI runs the target,
# so a local run and a CI run are the same commands. `make` runs the fast set (check).

# The shell of a GitHub Actions run step.
SHELL := bash
.SHELLFLAGS := -eo pipefail -c
MAKEFLAGS += --output-sync=target

TOOLS := $(CURDIR)/.tools
REQ := .github/requirements
LINTERS := .github/linters
export PATH := $(TOOLS)/venv/bin:$(TOOLS)/npm/node_modules/.bin:$(PATH)

# The range of `make commits`. CI sets it to the commits of the pull request.
RANGE ?= origin/main..HEAD

.PHONY: check tools lint prose rust deny verify sql test-cases l1 commits secrets
.DEFAULT_GOAL := check

check: lint prose test-cases rust

# The pinned lint and SQL tools (hashes in $(REQ)). Reinstalls when a pin changes.
tools: $(TOOLS)/stamp
$(TOOLS)/stamp: $(REQ)/tools.txt $(REQ)/package.json $(REQ)/package-lock.json
	rm -rf $(TOOLS)
	uv venv --quiet $(TOOLS)/venv
	uv pip install --quiet --python $(TOOLS)/venv/bin/python --require-hashes --no-deps -r $(REQ)/tools.txt
	mkdir -p $(TOOLS)/npm
	cp $(REQ)/package.json $(REQ)/package-lock.json $(TOOLS)/npm/
	npm ci --prefix $(TOOLS)/npm --ignore-scripts --no-audit --no-fund --silent
	touch $@

# Linters get tracked files only, so local drafts and build output stay out.
lint: tools
	git ls-files -z ':(glob)*.md' ':(glob)docs/**/*.md' ':(glob).github/**/*.md' \
	  | xargs -0 markdownlint-cli2 --config $(LINTERS)/.markdownlint-cli2.yaml
	git ls-files -z '*.yml' '*.yaml' | xargs -0 yamllint -c $(LINTERS)/.yamllint.yaml
	actionlint
	git ls-files -z '*.toml' | xargs -0 taplo fmt --check
	git ls-files -z 'scripts/*.sh' | xargs -0 -r shellcheck

# Templates (0000-*) stay out: they hold placeholder text.
prose:
	git ls-files 'docs/tech-design/*.md' 'docs/adr/*.md' | grep -v '/0000-' \
	  | xargs -r python3 .agents/skills/asd-ste100/scripts/ste-lint.py

# L0 runs once, under coverage. The floor is in CONSTRAINTS.md.
rust:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
	cargo llvm-cov --workspace --locked --fail-under-lines 75
	cargo test --workspace --doc --locked
	scripts/check-toolchain.sh

deny:
	cargo deny check

# The checks of tech design 0009. Needs Charon, Aeneas (scripts/install-aeneas.sh) and Lean.
verify:
	scripts/translate.sh --check
	cd lean && lake exe cache get
	python3 scripts/check-proofs.py
	REQUIRE_PROOF_TOOLS=1 python3 -m unittest discover -s scripts/tests

# Every .sql file must apply, in path order, to an empty Postgres (PGHOST, PGUSER).
sql: tools
	files=$$(git ls-files '*.sql' | sort); \
	echo "$$files" | xargs -r sqlfluff lint --dialect postgres; \
	for f in $$files; do psql -v ON_ERROR_STOP=1 -q -f "$$f"; done

# Each test case ID has a test or a PENDING marker.
test-cases:
	python3 -m unittest discover -s scripts/tests
	python3 scripts/check-test-cases.py

# L1: tests against the services of compose.yaml. The services stay up, so local data stays.
l1:
	docker compose up --wait
	scripts/dev-services-smoke.sh
	cargo test --workspace --locked --all-features

commits:
	cog check $(RANGE)

secrets:
	betterleaks git .
