# Spec Delta

## Purpose

Gives an operator the tasks that no client can do (tech designs 0003, 0021 and 0049). The tasks are for tenants, issuers, issuer bindings and the status of an account. The command does not use the API.

## ADDED Requirements

### Requirement: The operator command connects as the operator role

`strata admin` SHALL connect to PostgreSQL directly, not through the pooler, with the role `strata_operator`. The API and the worker SHALL not have the credentials of that role.

#### Scenario: The command uses the operator role

- **WHEN** an operator runs `strata admin tenant list`
- **THEN** the session of the command has the role `strata_operator`

### Requirement: Tenant commands

`strata admin tenant create <name>`, `tenant set-status <name> active|not-active` and `tenant list` SHALL create a tenant, change its status and list the tenants. A tenant name SHALL be 1 to 63 characters: lowercase letters `a` to `z`, digits and `-`, with a letter first. The command SHALL reject another name with an error and no write.

#### Scenario: A valid name creates an active tenant

- **WHEN** an operator runs `strata admin tenant create acme`
- **THEN** the tenant `acme` exists with the status active, and `tenant list` shows it

#### Scenario: A name that breaks the rule is rejected

- **WHEN** an operator runs `strata admin tenant create` with `Acme`, `1acme`, `acme_corp`, an empty name or a name of 64 characters
- **THEN** the command exits with an error, and no tenant exists

#### Scenario: Two tenants with one name

- **GIVEN** the tenant `acme`
- **WHEN** an operator runs `strata admin tenant create acme`
- **THEN** the command exits with an error, and one tenant `acme` exists

### Requirement: Issuer commands

`strata admin issuer register` SHALL add an issuer with the options of 0049: `--issuer`, `--jwks-url`, `--audience`, `--algorithm RS256|ES256|EdDSA`, `--tenant-claim`, and optionally `--account-claim` (default `sub`) and `--max-lifetime` (default 3600, from 1 to 3600). `strata admin issuer set <issuer>` SHALL change the same options and `--status active|not-active`.

#### Scenario: Register with the defaults

- **WHEN** an operator registers an issuer with no `--account-claim` and no `--max-lifetime`
- **THEN** the issuer has the account claim `sub`, the maximum lifetime 3600 and the status active

#### Scenario: A value out of range is rejected

- **WHEN** an operator registers an issuer with `--max-lifetime 0`, `--max-lifetime 3601` or `--algorithm HS256`
- **THEN** the command exits with an error, and no issuer exists

### Requirement: Binding commands

`strata admin binding add --issuer <issuer> --tenant-value <value> --tenant <name>` SHALL bind a tenant value of an issuer to a tenant. `strata admin binding remove --issuer <issuer> --tenant-value <value>` SHALL remove the binding. A tenant SHALL accept bindings from more than one issuer.

#### Scenario: Add and remove a binding

- **GIVEN** the issuer P1 and the tenant `acme`
- **WHEN** an operator adds a binding from `acme-corp` of P1 to `acme`, and then removes it
- **THEN** after the add the binding exists, and after the remove it does not exist

#### Scenario: One tenant value of one issuer binds to one tenant only

- **GIVEN** a binding from `acme-corp` of P1 to `acme`
- **WHEN** an operator adds a binding from `acme-corp` of P1 to `globex`
- **THEN** the command exits with an error, and the first binding does not change

### Requirement: Account status command

`strata admin account set-status --tenant <name> --issuer <issuer> --subject <sub> active|not-active` SHALL change the status of one account. An account that does not exist SHALL give an error and no write.

#### Scenario: Stop an account

- **GIVEN** an active account (P1, `u-1`) of `acme`
- **WHEN** an operator sets it to not active
- **THEN** the account is not active

#### Scenario: An account that does not exist

- **WHEN** an operator sets the status of an account that does not exist
- **THEN** the command exits with an error
