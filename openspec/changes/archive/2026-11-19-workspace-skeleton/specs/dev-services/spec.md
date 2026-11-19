# Spec Delta

## Purpose

Gives developers and the L1 job of CI the same local services: PostgreSQL and Garage, the S3 object storage of ADR 0005. One compose file starts both.

## ADDED Requirements

### Requirement: One command starts the services

`docker compose up --wait` in the repository root SHALL start PostgreSQL and Garage. The command SHALL return only when each service is healthy. If a service does not become healthy, the command SHALL fail.

#### Scenario: Both services become healthy

- **GIVEN** a developer machine with Docker and Compose v2, and no Strata containers
- **WHEN** the developer runs `docker compose up --wait`
- **THEN** the command returns with exit code 0
- **AND** PostgreSQL accepts a connection on `localhost:5432`
- **AND** Garage answers an S3 request on `localhost:3900`

### Requirement: Garage is ready for S3 requests at start

Garage SHALL start as a cluster of one node, with a bucket and an access key that the compose file states. A developer or a test SHALL not run a setup command before the first S3 request.

#### Scenario: Write and read an object

- **GIVEN** the services are healthy
- **WHEN** a client with the access key of the compose file writes an object to the bucket and reads it back
- **THEN** the client reads the same bytes

### Requirement: The services listen only on localhost

The compose file SHALL bind each port of each service to `127.0.0.1` only. The passwords and keys in the compose file are for development only, so another machine must not reach them.

#### Scenario: No port on all interfaces

- **WHEN** the services are healthy
- **THEN** no port of a Strata container is bound to `0.0.0.0` or `::`

### Requirement: CI runs L1 tests against the same services

The L1 job of CI SHALL run on each pull request. It SHALL start the services with the compose file of the repository and run the L1 tests. The L0 job SHALL not run L1 tests. An L1 test SHALL fail when a service is not available. It SHALL not skip.

#### Scenario: L1 job starts the services

- **WHEN** CI runs for a pull request
- **THEN** the L1 job starts the services with `docker compose up --wait`, and runs the L1 tests

#### Scenario: L0 job does not need the services

- **WHEN** the L0 job runs with no services
- **THEN** the L0 job runs no L1 test, and passes
