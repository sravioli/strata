# Tasks

## 1. Workspace and CodeQL

- [x] 1.1 Add the root `Cargo.toml` as a virtual workspace (`resolver = "3"`, edition 2024, `members = ["crates/*"]`). Add `crates/core` with the package name `strata-core`, `publish = false`, a `lib.rs` with a crate doc comment and `#![forbid(unsafe_code)]`, and no functions. The repository has no licence, so add `[licenses.private] ignore = true` to `deny.toml`, so that cargo-deny does not reject the crate for a missing `license` field. Add no `rust-toolchain` file: the crates build on stable (0009). Commit `Cargo.lock`. Verify: `cargo build --workspace --locked` and `cargo test --workspace --locked` pass on stable, and `cargo deny check` passes.
- [x] 1.2 In `.github/workflows/codeql.yml`, add `rust` to the `language` list, and remove the comment that says to wait for the first crate. The init step already sets `build-mode: none`. Verify: actionlint passes, and the CodeQL run of the pull request has a green `rust` leg.
- [x] 1.3 In the `rust` job of `.github/workflows/ci.yml`, add `--all-features` to the `cargo clippy` command, so that clippy also reads L1 test code. Keep `cargo test --workspace --locked` with no features: this is the L0 run. Verify: the `rust` and `deny` jobs pass on the pull request.

## 2. Local services

- [x] 2.1 Add `dev/garage.toml` for one node: `replication_factor = 1`, `db_engine = "sqlite"`, `metadata_dir` and `data_dir` on a volume, a fixed `rpc_secret` with a comment that says it is for development only, and `[s3_api]` with `s3_region = "garage"` and `api_bind_addr = "[::]:3900"`. Verify: Garage starts with the file in task 2.2.
- [x] 2.2 Add `compose.yaml` with two services:
  - `postgres`: image `postgres:17`, the tag that the `sql` job of CI uses, pinned by digest. A development password, port `127.0.0.1:5432:5432`, a named volume, and the healthcheck `pg_isready`.
  - `garage`: image `dxflrs/garage:v2.5.0` pinned by digest. The command `/garage server --single-node --default-bucket`, with `GARAGE_DEFAULT_ACCESS_KEY`, `GARAGE_DEFAULT_SECRET_KEY` and `GARAGE_DEFAULT_BUCKET` set to development values. Mount `dev/garage.toml` at `/etc/garage.toml`, the default path that `/garage status` also reads. Port `127.0.0.1:3900:3900`, named volumes, and the healthcheck `["CMD", "/garage", "bucket", "info", "strata"]` (`garage status` exits 0 also with no layout, so it is not a healthcheck).

  The image has no shell, so the healthcheck uses the exec form. Verify that Garage accepts the format of the access key ID. Verify that the healthcheck passes only when S3 requests succeed. If `garage status` exits 0 too early, use a different healthcheck and record the reason in a comment. Verify: on a clean machine, `docker compose up --wait` returns 0.
- [x] 2.3 Add `scripts/dev-services-smoke.sh`. It tests the scenarios of the `dev-services` spec:
  - `psql` connects to `localhost:5432`.
  - The `aws` CLI with the keys of `compose.yaml` writes an object to the bucket, reads it back and compares the bytes.
  - `docker compose ps --format json` shows no port on `0.0.0.0` or `::`.

  The script exits non-zero at the first failure. Verify: shellcheck passes, and the script passes after task 2.2.
- [x] 2.4 Add the `l1` job to `.github/workflows/ci.yml`. It runs only for `pull_request`, needs no filter of `changes`, and does these steps:
  1. `docker compose up --wait`
  2. `scripts/dev-services-smoke.sh`
  3. `cargo test --workspace --locked --all-features`
  4. `docker compose down -v`, also on failure

  Add `l1` to `needs` of `ci-ok`. Verify: actionlint and zizmor pass, and the `l1` job is green on the pull request.

## 3. Test case check

- [x] 3.1 Add `scripts/check-test-cases.py` (Python standard library only, as `ste-lint.py`). It implements each requirement of the `test-case-check` spec:
  - It reads the IDs (`TC-<issue>-<n>`) from the test case tables of `docs/tech-design/*.md` (not `0000-*`), from the scenario names in `openspec/specs/**/spec.md`, and from `openspec/changes/*/specs/**/spec.md` (not `archive/`).
  - It reads the IDs that tests carry from `git ls-files`, for files that are not Markdown and are outside `docs/` and `openspec/`, not the unit test file of the check itself, and with no `fixtures` segment in the path. Thus the example IDs of the check's own tests, and the fixtures of change 2, do not count. The other tests in `scripts/tests/`, for example the tests of change 2 that carry TC-9-01 to TC-9-06, count.
  - The `pending` marker of a scenario is a step line `- **PENDING** <reason>` in the scenario.
  - Each failure line names the ID and the file. The exit code is 1 when a failure exists, and 0 when no failure exists.

  Verify: `openspec validate --strict` accepts a spec that contains the marker line, and the script passes on the repository as it is now.
- [x] 3.2 Add `scripts/tests/test_check_test_cases.py` (`unittest`), with each test building its own small file tree in a temporary git repository, one test for each scenario of the `test-case-check` spec. Verify: `python3 -m unittest discover -s scripts/tests` passes, and each test fails when you remove its rule from the script.
- [x] 3.3 Add the `test-cases` job to `.github/workflows/ci.yml`. It runs on each run, needs no filter, and runs the unit tests of task 3.2 and then the check. Add it to `needs` of `ci-ok`. Verify: actionlint passes, and the job is green on the pull request.

## 4. Documentation

- [x] 4.1 Update `docs/agents/testing.md`:
  - An L1 test target has `required-features = ["l1"]` in its `Cargo.toml`. Run L1 tests with `docker compose up --wait` and then `cargo test --workspace --all-features`.
  - After a test case gets its spec scenario, the spec is its only status. The tech design row is not updated.
  - The `pending` marker is the line `- **PENDING** <reason>` in the scenario.
  - `scripts/check-test-cases.py` enforces these rules.

  Verify: ste-lint passes on the file, markdownlint passes, and each command in the text runs as written.

## 5. Integration

- [x] 5.1 Run `npx markdownlint-cli2` on the changed Markdown files, then open one pull request with all groups. Its body says `Refs #11`. Verify: all CI jobs, the CodeQL `rust` leg and `ci-ok` are green.

## Workflow follow-up

- The owner merges the pull request.
- Archive this change with `openspec-archive-change`.
- Start change 2 of issue #11 with `openspec-propose`. It covers the checks of 0009, the Lean project, `authorize`, `authorized_scope`, `job_step` and their theorems. It closes #11.
