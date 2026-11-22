# The checks of CI. Each target is one job of .github/workflows/ci.yml, and CI runs the target,
# so a local run and a CI run are the same commands. `make` runs the fast set (check).
# `make setup` installs every pinned tool into .tools/ and checks for the toolchains.

# The shell of a GitHub Actions run step.
SHELL := bash
.SHELLFLAGS := -eo pipefail -c
MAKEFLAGS += --output-sync=target

TOOLS := $(CURDIR)/.tools
REQ := .github/requirements
LINTERS := .github/linters
BIN := $(TOOLS)/bin
AENEAS := $(TOOLS)/aeneas
export PATH := $(BIN):$(AENEAS):$(TOOLS)/venv/bin:$(TOOLS)/npm/node_modules/.bin:$(PATH)

# The range of `make commits`. CI sets it to the commits of the pull request.
RANGE ?= origin/main..HEAD

.PHONY: check setup tools lint prose rust deny verify sql test-cases l1 commits secrets
.DEFAULT_GOAL := check

check: lint prose test-cases rust

# Toolchains that manage themselves stay out of .tools/: setup only checks for them.
setup: tools $(BIN)/cargo-llvm-cov $(BIN)/cargo-deny $(BIN)/cog $(BIN)/betterleaks $(AENEAS)/charon
	@for c in cargo rustup; do command -v $$c >/dev/null || { echo "missing: $$c (make check needs it)" >&2; exit 1; }; done
	@for t in verify:lake l1:docker sql:psql; do command -v $${t#*:} >/dev/null || echo "warning: no $${t#*:}, so make $${t%%:*} cannot run"; done
	@echo "setup ok"

# The pinned lint and SQL tools (hashes in $(REQ)). Reinstalls when a pin changes.
tools: $(TOOLS)/stamp
$(TOOLS)/stamp: $(REQ)/tools.txt $(REQ)/package.json $(REQ)/package-lock.json
	rm -rf $(TOOLS)/venv $(TOOLS)/npm
	uv venv --quiet $(TOOLS)/venv
	uv pip install --quiet --python $(TOOLS)/venv/bin/python --require-hashes --no-deps -r $(REQ)/tools.txt
	mkdir -p $(TOOLS)/npm
	cp $(REQ)/package.json $(REQ)/package-lock.json $(TOOLS)/npm/
	npm ci --prefix $(TOOLS)/npm --ignore-scripts --no-audit --no-fund --silent
	touch $@

# The pinned release binaries (URL and SHA-256 in $(REQ)/tools.env).
$(BIN)/%: $(REQ)/tools.env scripts/install-tool.sh
	scripts/install-tool.sh $* $(BIN)

# The pinned Aeneas and Charon (toolchain.env). Charon also needs the nightly CHARON_RUST of toolchain.env.
$(AENEAS)/charon: toolchain.env scripts/install-aeneas.sh
	scripts/install-aeneas.sh $(AENEAS)
	. ./toolchain.env && $@ version | grep -F "$$CHARON_COMMIT"
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

# L0, with the doc tests. The coverage floor of CONSTRAINTS.md runs in l1, over L0 and L1.
rust:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
	cargo test --workspace --locked
	scripts/check-toolchain.sh

deny: $(BIN)/cargo-deny
	cargo deny check

# The checks of tech design 0009. Needs Lean (elan) and the nightly CHARON_RUST of toolchain.env.
verify: $(AENEAS)/charon
	scripts/translate.sh --check
	cd lean && lake exe cache get
	python3 scripts/check-proofs.py
	REQUIRE_PROOF_TOOLS=1 python3 -m unittest discover -s scripts/tests

# Every .sql file must apply, in path order, to an empty Postgres (PGHOST, PGUSER).
sql: tools
	files=$$(git ls-files '*.sql' | sort); \
	echo "$$files" | xargs -r sqlfluff lint --dialect postgres; \
	psql -v ON_ERROR_STOP=1 -q -f dev/roles.sql; \
	for f in $$(echo "$$files" | grep -vx dev/roles.sql); do psql -v ON_ERROR_STOP=1 -q -f "$$f"; done

# Each test case ID has a test or a PENDING marker.
test-cases:
	python3 -m unittest discover -s scripts/tests
	python3 scripts/check-test-cases.py

# L1: tests against the services of compose.yaml. The services stay up, so local data stays.
# L0 and L1 run together under coverage. The floor is in CONSTRAINTS.md.
l1: $(BIN)/cargo-llvm-cov
	docker compose up --wait
	scripts/dev-services-smoke.sh
	cargo llvm-cov --workspace --locked --all-features --fail-under-lines 80

commits: $(BIN)/cog
	cog check $(RANGE)

secrets: $(BIN)/betterleaks
	betterleaks git .
