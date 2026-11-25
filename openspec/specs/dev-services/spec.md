# dev-services Specification

## Purpose

Gives developers and the L1 job of CI the same local services: PostgreSQL and Garage, the S3 object storage of ADR 0005. One compose file starts both.

## Requirements

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

### Requirement: PgBouncer pools in transaction mode

PgBouncer SHALL run in transaction mode, with `max_prepared_statements` set to 200, in front of the database of Strata. The runtime roles `strata_api` and `strata_worker` SHALL connect through it.

#### Scenario: A runtime role connects through PgBouncer

- **GIVEN** the services are healthy
- **WHEN** `strata_api` connects on `localhost:6432` and runs a prepared statement in a transaction
- **THEN** the statement succeeds

### Requirement: The roles exist when PostgreSQL starts

A new data folder of PostgreSQL SHALL get the four roles of 0003 at its first start: `strata_owner`, `strata_api`, `strata_worker` and `strata_operator`. Each role SHALL log in with a development password that is the name of the role. `dev/roles.sql` SHALL set these passwords. No tracked file SHALL hold a database URL with a password. Only `strata_operator` SHALL bypass row-level security.

#### Scenario: The roles exist after the first start

- **GIVEN** no volume of PostgreSQL
- **WHEN** the developer runs `docker compose up --wait`
- **THEN** each of the four roles can log in with its development password

### Requirement: The setup builds the pinned nsjail

`make setup` SHALL build nsjail 3.6 from source at the pinned commit `f78475530b46d0186111a9096b30725f816b55fe`, into `.tools/`. The build SHALL stop when the commit of the source is not the pin. `make setup` SHALL warn when `pdftotext` is missing. The `l1` job of CI SHALL use the same build.

#### Scenario: The pinned build

- **WHEN** `make setup` runs on a system with the build dependencies of nsjail
- **THEN** `.tools/` holds an nsjail binary built from the pinned commit

#### Scenario: A source that is not the pin

- **GIVEN** a source of nsjail at a commit that is not the pin
- **WHEN** the build script runs
- **THEN** it stops with an error, and it installs no binary

### Requirement: CI runs the sandbox tests

The `l1` job of CI SHALL run the L1 tests of the sandbox with nsjail. If the runner blocks the user namespaces of nsjail, the job SHALL load an AppArmor profile that permits user namespaces for the nsjail binary only. The job SHALL NOT set `kernel.apparmor_restrict_unprivileged_userns` to 0.

#### Scenario: The sandbox tests run in CI

- **WHEN** CI runs the `l1` job for a pull request
- **THEN** the L1 tests of the sandbox run with nsjail, and none of them skips

#### Scenario: No global change to the restriction

- **WHEN** a test reads `.github/workflows/ci.yml` and the scripts that it runs
- **THEN** no step sets `kernel.apparmor_restrict_unprivileged_userns` to 0

### Requirement: The L1 tests run in a delegated cgroup

`make l1` SHALL run the tests in a delegated cgroup, and SHALL give its path in `STRATA_CGROUP`. Locally, it SHALL use `systemd-run --user --scope -p Delegate=yes`. In CI, the step that runs `make l1` SHALL create `/sys/fs/cgroup/strata` with `sudo`, give it to the user of the job, and move its shell into a leaf child.

#### Scenario: The memory limit works in the tests

- **WHEN** `make l1` runs the test of the memory limit, locally or in CI
- **THEN** the test runs with the control group limits, and it does not skip

### Requirement: One command starts the walking skeleton

`make dev` SHALL start the services, apply the migrations as `strata_owner`, and start `strata-dev serve`. It SHALL create the tenants `acme` and `globex`, register the development issuer, and add one binding for each tenant. Then it SHALL start `strata api` and `strata worker` on the host. The worker SHALL run in a delegated cgroup. A second run SHALL not fail on the tenants, the issuer and the bindings that exist. `make dev` SHALL stop the processes that it started when the developer stops it.

#### Scenario: The first run

- **GIVEN** a developer machine with the services stopped and an empty database
- **WHEN** the developer runs `make dev`
- **THEN** the API answers on `127.0.0.1:8080`, and a token of `strata-dev token --tenant acme --sub alice` gets `200` from a search

#### Scenario: A second run

- **GIVEN** a database where an earlier run of `make dev` created the tenants and the bindings
- **WHEN** the developer runs `make dev` again
- **THEN** it starts the API and the worker, and fails on no step

### Requirement: The L2 tests run against the walking skeleton

`make l2` SHALL start the processes of `make dev` in the background, run the L2 tests over HTTP, and stop the processes, also when a test fails. The L2 tests SHALL be test targets with the feature `l2`, so that `make rust` and `make l1` do not run them. An L2 test SHALL fail when the API does not answer. It SHALL not skip.

#### Scenario: TC-49-01 From upload to search locally

- **GIVEN** the walking skeleton locally, with the tenants A and B and an account of each
- **WHEN** the account of A uploads a PDF with a unique word, and polls the status of the revision until it is final
- **THEN** the status is `ready`
- **AND** a search of A for the word returns the document
- **AND** the same search of B returns no result

#### Scenario: The processes stop after a failed test

- **GIVEN** an L2 test that fails
- **WHEN** `make l2` ends
- **THEN** no process of the API, the worker or `strata-dev` that `make l2` started runs

### Requirement: CI runs the L2 tests

The `l2` job of CI SHALL run on each pull request after the `l1` job. It SHALL have the same services, nsjail, delegated cgroup and `pdftotext` as the `l1` job, and SHALL run `make l2`. The job that gates the pull request SHALL need the `l2` job.

#### Scenario: The l2 job runs after l1

- **WHEN** CI runs for a pull request
- **THEN** the `l2` job starts after the `l1` job passes, and runs `make l2`
- **AND** a failed `l2` job fails the gate of the pull request

### Requirement: The observability services are a compose profile

`compose.yaml` SHALL have the services of the collector, of Grafana with Tempo, Loki and Prometheus, and of the PgBouncer exporter in the profile `observability`. `docker compose up --wait` without the profile, and `make l1`, SHALL not start them. Each image SHALL be pinned by tag and digest. Each port SHALL bind to `127.0.0.1`. Garage SHALL expose its metrics on its admin port to the collector.

#### Scenario: The default start leaves the profile out

- **WHEN** a developer runs `docker compose up --wait` with no profile
- **THEN** PostgreSQL, PgBouncer and Garage start, and no service of the profile `observability` starts

#### Scenario: The profile starts and listens on localhost

- **WHEN** a developer starts the services with the profile `observability`
- **THEN** the collector accepts OTLP/HTTP on `127.0.0.1:4318`
- **AND** Grafana answers on `127.0.0.1:3000`
- **AND** no port of the profile is bound to `0.0.0.0` or `::`

### Requirement: make dev turns the telemetry on, except in CI

`make dev` SHALL start the services of the profile `observability`, and SHALL set `OTEL_EXPORTER_OTLP_ENDPOINT` to `http://127.0.0.1:4318` for `strata api` and `strata worker`. When the variable `CI` is set, `make dev` and `make l2` SHALL start no service of the profile and set no endpoint.

#### Scenario: A local run

- **GIVEN** the variable `CI` is not set
- **WHEN** a developer runs `make dev`
- **THEN** the services of the profile run, and the API and the worker export to the collector

#### Scenario: A run in CI

- **GIVEN** the variable `CI` is set
- **WHEN** the `l2` job of CI runs `make l2`
- **THEN** no service of the profile starts, and `OTEL_EXPORTER_OTLP_ENDPOINT` is not set for the API and the worker
