# Spec Delta

## Purpose

Gives a developer and the tests an issuer of access tokens on a laptop, with no external identity provider (tech design 0021, "Development issuer").

## ADDED Requirements

### Requirement: The server does not contain the development issuer

`strata-dev` SHALL be a separate crate. The dependencies of the server crate SHALL not include it, directly or through another crate. This includes the dependencies of its tests. The L1 tests SHALL be in a separate crate that depends on both.

#### Scenario: TC-21-17 The dependency graph has no development issuer

- **WHEN** a test reads the dependency graph of the server crate
- **THEN** it does not contain the `strata-dev` crate
- **AND** the build fails if it does
- **PENDING** change 1 of #49 adds the test

### Requirement: The development issuer keeps one key pair

`strata-dev` SHALL make an Ed25519 key pair on its first run and keep it in a local file that only the user can read. Each later run SHALL use the same key pair.

#### Scenario: Same key on the second run

- **GIVEN** a first run of `strata-dev` that made a key pair
- **WHEN** `strata-dev` runs again
- **THEN** it serves the same public key

### Requirement: The development issuer serves its JWKS on loopback only

`strata-dev serve` SHALL serve the JWKS on `127.0.0.1` only, and print the `strata admin` commands that register the issuer with the algorithm `EdDSA`.

#### Scenario: Serve and print

- **WHEN** a developer runs `strata-dev serve`
- **THEN** the JWKS answers on `127.0.0.1` and on no other address
- **AND** the output holds a `strata admin issuer register` command with its JWKS URL

### Requirement: The development issuer issues tokens

`strata-dev token --tenant <value> --sub <subject>` SHALL print a signed token with `iss`, `aud`, `sub`, the tenant claim, `iat` and `exp`. An option SHALL set the lifetime. The default lifetime SHALL be 3600 seconds.

#### Scenario: The API accepts the token

- **GIVEN** the development issuer is registered and bound to tenant `acme` for the value `acme`
- **WHEN** a test gets a token with `--tenant acme --sub alice` and sends it to the API
- **THEN** the API acts for the account (development issuer, `alice`) of `acme`
