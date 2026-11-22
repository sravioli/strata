# Spec Delta

## MODIFIED Requirements

### Requirement: One command starts the services

`docker compose up --wait` in the repository root SHALL start PostgreSQL 19, PgBouncer and Garage. The command SHALL return only when each service is healthy. If a service does not become healthy, the command SHALL fail.

#### Scenario: Both services become healthy

- **GIVEN** a developer machine with Docker and Compose v2, and no Strata containers
- **WHEN** the developer runs `docker compose up --wait`
- **THEN** the command returns with exit code 0
- **AND** PostgreSQL accepts a connection on `localhost:5432`
- **AND** Garage answers an S3 request on `localhost:3900`

#### Scenario: PostgreSQL 19 and PgBouncer become healthy

- **GIVEN** a developer machine with Docker and Compose v2, and no Strata containers
- **WHEN** the developer runs `docker compose up --wait`
- **THEN** `select version()` on `localhost:5432` shows PostgreSQL 19
- **AND** PgBouncer accepts a connection on `localhost:6432`

## ADDED Requirements

### Requirement: PgBouncer pools in transaction mode

PgBouncer SHALL run in transaction mode, with `max_prepared_statements` set to 200, in front of the database of Strata. The runtime roles `strata_api` and `strata_worker` SHALL connect through it.

#### Scenario: A runtime role connects through PgBouncer

- **GIVEN** the services are healthy
- **WHEN** `strata_api` connects on `localhost:6432` and runs a prepared statement in a transaction
- **THEN** the statement succeeds

### Requirement: The roles exist when PostgreSQL starts

A new data folder of PostgreSQL SHALL get the four roles of 0003 at its first start: `strata_owner`, `strata_api`, `strata_worker` and `strata_operator`. Each role SHALL log in with a development password that is the name of the role. No tracked file SHALL hold a password as the value of a setting. Only `strata_operator` SHALL bypass row-level security.

#### Scenario: The roles exist after the first start

- **GIVEN** no volume of PostgreSQL
- **WHEN** the developer runs `docker compose up --wait`
- **THEN** each of the four roles can log in with its development password
