# Spec Delta

## ADDED Requirements

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
