Issue: #49

# Proposal

## Why

Tech design 0049 builds the walking skeleton in four changes. This is change 1. It adds the base of the other three changes. That base is the schema of tenants and accounts, the database roles, the verification of tokens, and the operator command. No route of a document can exist before an API can find the tenant and the account of a caller.

## What Changes

- Move PostgreSQL from 17 to 19 in `compose.yaml` and in the `sql` job of CI, in one commit. If PostgreSQL 19 does not work with an image or with PgBouncer, use PostgreSQL 18 and record the reason (0049, "Identifiers").
- Add PgBouncer in transaction mode to `compose.yaml`, with `max_prepared_statements` set to 200 (0049, "Local services").
- Create the four roles of 0003 with an initialization script that runs as the superuser of the container. `strata_owner` cannot create roles.
- Add the first migrations: `tenants` and `accounts` (0003), and `issuers` and `issuer_bindings` (0021). Each identifier comes from `uuidv7()`. Each table gets its policy and its grants.
- Change the identifiers of a tenant and of an account in `core` from `u64` to `u128`. Translate `core` again and update the proofs in the same pull request (0049, "Identifiers").
- Add the crate `strata`, a library and a binary with the subcommands `api`, `admin` and `migrate`:
  - `strata migrate` applies the migrations that refinery embeds, as `strata_owner`.
  - `strata admin` has the commands for tenants, issuers, bindings and accounts (0049, "Operator commands"). It connects as `strata_operator`.
  - `strata api` verifies each token with the 10 steps of 0021, and opens the identity transaction and the tenant transaction of 0004.
- Add the types `IdentityTx`, `Allowed` and `TenantTx`, and the test helper `tenant_tx` (0049, "Identity, decisions and transactions in Rust").
- Add the crate `strata-dev`, the development issuer of 0021. The `strata` crate does not depend on it (TC-21-17).
- Add the crate `strata-tests` for the L1 tests. It depends on `strata` and `strata-dev`, so the tests can sign tokens and `strata` keeps no dependency on `strata-dev`. It is not in the crate table of 0049, because it holds only tests.
- Set the pool of `strata api` to 20 connections and a wait of 2 seconds. After the wait, the API answers `503` with `Retry-After: 1` (0049, "Values").

Not in this change:

- `strata worker`, `ClaimTx` and `strata admin job requeue`. They need the table `jobs`, which change 2 adds. Change 3 adds the worker.
- A public route. No design gives a route that only authenticates. The L1 tests check the authentication through a router that exists only in the tests.
- `make dev`. Change 4 adds it.
- TC-49-10. Change 3 adds the nsjail configuration that it tests.

## Decisions for this change

- The owner decided the status codes on 2026-11-22. Each failure of steps 1 to 9 of 0021 gets `401`. An account or a tenant that is not active (step 10) gets `403`. This change amends 0021 with this decision.
- The owner approved two more values on 2026-11-22. A `401` has the header `WWW-Authenticate: Bearer`, because RFC 9110 requires a `WWW-Authenticate` header on a `401`. Each rejection has an empty body, so that the body shows no step.
- The owner decided on 2026-11-22: the coverage floor of `CONSTRAINTS.md` counts the L0 and L1 tests together, in the `l1` job. With L0 tests only, the database and HTTP code of `strata` gave 47 %. The owner set the number to 85. The new value is 86.52 %.
- The grants for the four tables of this change come from 0003 and 0021. The spec `tenant-isolation` lists them.

## Test cases

This change carries the test cases that it can make true, each as a scenario with its TC ID:

- 0003: TC-3-01, TC-3-02, TC-3-03, TC-3-05, TC-3-06, TC-3-07, TC-3-09, TC-3-11, TC-3-13, TC-3-15, TC-3-16, TC-3-20.
- 0004: TC-4-04.
- 0021: TC-21-01 to TC-21-18. TC-21-18 is L2, so its scenario keeps the pending marker.
- 0002: TC-2-06.

These test cases stay in their designs with the status `pending`, and a later change carries them:

| Test cases | Reason | Change |
|---|---|---|
| TC-3-04 | It needs a child table with a composite foreign key: `revisions` of `documents`. | 2 |
| TC-3-18, TC-4-08 | They check each route. With no route, the check proves nothing. | 2 |
| TC-3-12, TC-4-01, TC-4-02, TC-4-03, TC-4-09 | They need the routes of a document. | 2 and 4 |
| TC-3-17, TC-4-07, TC-4-10 | They need a job and the worker. | 3 |
| TC-3-10, TC-3-19, TC-4-05, TC-4-06 | They need the search. | 4 |
| TC-2-07 | The skeleton has no read replica. | M3 ([#61](https://github.com/sravioli/strata/issues/61)) |

## Capabilities

### New Capabilities

- `tenant-isolation`: the rules of a tenant-owned table, the policies, the roles and their grants, the tenant transaction and the pooler.
- `request-authentication`: the verification of a token, the key cache, the identity transaction, the account on the first request, and the responses of a rejection.
- `operator-command`: `strata admin` for tenants, issuers, bindings and accounts, and the rule for a tenant name.
- `schema-migrations`: `strata migrate` and the migrations in the binary.
- `dev-issuer`: `strata-dev`, its keys, its JWKS and its tokens.

### Modified Capabilities

- `dev-services`: the services are PostgreSQL 19, PgBouncer and Garage. The four roles of 0003 exist when PostgreSQL starts.

## Impact

- New crates: `crates/strata`, `crates/strata-dev` and `crates/strata-tests`. New SQL files in `crates/strata/migrations/`.
- `crates/core/src/authorization.rs`: `u64` becomes `u128` for the identifiers. `lean/Strata/Generated/` and `lean/Strata/Proofs/Authorization.lean` change with it.
- New dependencies: axum, tokio, tokio-postgres, deadpool-postgres, refinery, jsonwebtoken, clap, an HTTP client for the JWKS, serde, uuid and tracing. `cargo deny` checks each one.
- `compose.yaml`, `dev/`, `.github/workflows/ci.yml` and `Makefile`: PostgreSQL 19, PgBouncer and the roles.
- `docs/tech-design/0021-identity-providers.md`: an amended note with the status codes of a rejection.
- No data exists, so no migration of data is necessary.
