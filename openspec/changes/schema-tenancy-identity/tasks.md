# Tasks

## 1. Local services and CI

- [x] 1.1 Find the image `postgres:19` and pin its digest in `compose.yaml`. Move the image of the `sql` job in `.github/workflows/ci.yml` to the same version in the same commit. If PostgreSQL 19 does not work with the image or with PgBouncer, use 18 in both places. Write the reason in the commit message and in the pull request. Verify: `docker compose up --wait` passes, and `select version()` shows the pinned version.
- [x] 1.2 Add `dev/roles.sql`, which PostgreSQL runs at the first start of a new data folder, as the superuser. It creates `strata_owner`, `strata_api`, `strata_worker` and `strata_operator`, with the attributes of 0003. Only `strata_operator` gets `bypassrls`. Each development password is the name of its role, set with `format`, so that no tracked file holds a password value. `strata_owner` owns the database, because from PostgreSQL 15 PUBLIC has no `CREATE` on `public`. The `sql` job of CI and `make sql` run the same role setup before the migrations. Verify also: `make sql` passes against an empty PostgreSQL. Verify: after `docker compose down -v` and `docker compose up --wait`, each role logs in.
- [x] 1.3 Add PgBouncer to `compose.yaml`: a pinned image, transaction mode, `max_prepared_statements = 200`, port `127.0.0.1:6432`, and a health check. Add a second pool of one server connection for TC-3-05. Extend `scripts/dev-services-smoke.sh` for PgBouncer and the roles. Verify: the scenarios of the `dev-services` delta spec pass with that script.
- [x] 1.4 Write no URL that holds a password in a tracked file (the failure of the `secrets` job on #100). Verify: `make secrets` passes with no new entry in `.betterleaksignore`.

## 2. Identifiers of 128 bits in core

- [x] 2.1 In `crates/core/src/authorization.rs`, change `u64` to `u128` for the identifiers of a tenant and of an account, in `Principal`, `Resource`, `Scope` and their functions. Update the L0 tests. Verify: `cargo test -p strata-core` passes.
- [x] 2.2 Run `scripts/translate.sh` with the pinned Charon and Aeneas, and update `absPrincipal`, `absResource` and the other abstractions in `lean/Strata/Proofs/Authorization.lean`. The statements of the theorems do not change. If Charon or Aeneas cannot translate `u128`, stop and tell the owner. Verify: `make verify` passes with no `sorry`.

## 3. Crates and dependencies

- [x] 3.1 Add `crates/strata`: a library, and a binary with the `clap` subcommands `api`, `admin` and `migrate`. Add `crates/strata-dev`: a library and a binary. Add `crates/strata-tests` for the L1 tests, with `required-features = ["l1"]`. It depends on `strata` and `strata-dev`. Add the dependencies of the proposal at exact versions. The JWKS client uses rustls. Verify: `cargo build --workspace --locked` and `make deny` pass.
- [x] 3.2 Add a test for TC-21-17 that reads `cargo metadata` and fails when a dependency of `strata`, of any kind, reaches `strata-dev`. Verify: the test passes. It fails when `strata-dev` is added to `[dependencies]` or `[dev-dependencies]` of `strata`. Remove the pending marker of TC-21-17.
- [x] 3.3 Read the database URL of each subcommand from one environment variable. The process reads no other source of credentials. Verify: a subcommand with no variable exits with an error that names the variable.

## 4. Schema and migrations

- [x] 4.1 Add the first migration in `crates/strata/migrations/`. It holds `tenants`, `accounts`, `issuers` and `issuer_bindings`, with the columns of 0003 and 0021. It also holds the `uuidv7()` defaults, the three rules of 0003, the policies, and the grants of the `tenant-isolation` spec. Copy the expressions of the policies from 0003 and 0021 without a change. Verify: `make sql` passes.
- [x] 4.2 Add `strata migrate` with refinery, which embeds the migrations. It connects directly as `strata_owner`. Add the L1 tests of the `schema-migrations` spec. Verify: the tests pass.
- [x] 4.3 Add the L1 tests of the catalog: TC-3-06 and TC-3-07, and the test of the list of grants. Each test reads all tables and roles from the catalog. The list of global tables is in one place in the test code. It holds `issuers` and the history table of refinery. Verify: the tests pass, and each fails when a policy, a grant or a role attribute changes. Remove the pending markers.
- [x] 4.4 Add the L1 tests TC-3-01, TC-3-02, TC-3-03, TC-3-15, TC-3-20 and TC-21-15. Also add the tests of version 7 identifiers and of the update of the account status. Use `accounts` as the tenant-owned table. Verify: the tests pass. Remove the pending markers.

## 5. Transactions in Rust

- [x] 5.1 Add the pool of deadpool-postgres with 20 connections and a wait of 2 seconds. Add `TenantTx`, `IdentityTx` and `Allowed`, with `allowed(Decision) -> Option<Allowed>` and `IdentityTx::into_tenant`. `into_tenant` takes `Allowed` or a `Scope`, and sets `app.tenant_id` with `set_config(…, true)`. Follow the pipelining rule of 0049. Add the test helper `tenant_tx`. Verify: a `compile_fail` doc test in the library of `strata` shows that a query function does not accept a pool or a connection.
- [x] 5.2 Add the L1 test TC-3-05 through the pool of one connection, and the test of prepared statements through PgBouncer. Verify: the tests pass. Remove the pending marker of TC-3-05.

## 6. Operator command

- [ ] 6.1 Add `strata admin` with the commands for tenants, issuers, bindings and accounts of 0049. It connects directly as `strata_operator`. Put the rule for a tenant name and the verification of the options in pure functions with L0 tests. Verify: `cargo test -p strata` passes, and the L1 tests of the `operator-command` spec pass.

## 7. Development issuer

- [ ] 7.1 In `strata-dev`, add the key pair in a file with mode `0600`, `serve` on `127.0.0.1`, `token`, and the printed commands that register the issuer. The token functions are in the library, so the L1 tests of `strata` can sign tokens in their process. Verify: the L1 tests of the `dev-issuer` spec pass.

## 8. Authentication of the API

- [ ] 8.1 Add the claim verification of step 6 of 0021 as pure functions, with L0 tests for each claim and each limit. Verify: `cargo test -p strata` passes.
- [ ] 8.2 Add the key cache of each issuer, with the rules of "Signing keys" in 0021. A fake JWKS server in the tests counts its requests. Verify: the L1 tests TC-21-10, TC-21-11 and TC-21-12 pass. Remove their pending markers.
- [ ] 8.3 Add the authentication layer of `strata api` with steps 1 to 10 of 0021. Steps 1 to 9 give `401` with `WWW-Authenticate: Bearer`. An account or a tenant that is not active gives `403`. Each has an empty body. The end of the pool wait gives `503` with `Retry-After: 1`. Log `act.sub`. Steps 4 to 6 hold no connection. Verify: the L1 tests of the `request-authentication` spec pass with a router that exists only in the tests. Remove each pending marker except the one of TC-21-18.
- [ ] 8.4 For TC-3-13 and TC-3-09, the test must show that no query on a tenant-owned table runs. Use the statistics of PostgreSQL for each table, or a log of statements, and record the method in the test. Verify: the test fails when the layer reads `accounts` before it verifies the signature.
- [ ] 8.5 Amend 0021, "Verification of a token": a failure of steps 1 to 9 gives `401` with `WWW-Authenticate: Bearer`, and step 10 gives `403`, each with an empty body. Add the note `Amended on <date> (#49)`. Verify: `make lint prose` passes.

## 9. Integration and gates

- [ ] 9.1 Verify the coverage floor of `CONSTRAINTS.md`. The L1 code of `strata` does not count in the L0 coverage. If the workspace falls below 80 %, stop and tell the owner, with the numbers. Do not lower the number. Verify: `cargo llvm-cov --workspace --locked --fail-under-lines 80` passes.
- [ ] 9.2 Run `make` and `make l1`. Verify: each target passes, and `python3 scripts/check-test-cases.py --report` shows each scenario of this change as `implemented`, except TC-21-18 as `planned`.
- [ ] 9.3 Update the implementation-point lists in the comments of #49. Tick each point that this change implements, and name the change that implements each other point. Verify: each point of #3, #4 and #21 that change 1 owns is ticked.

## Workflow follow-up

- Open the pull request with the `pr` skill, and add it to project 1.
- Archive the change with `/openspec-archive-change` after the owner merges the pull request.
