Issue: #133

# Proposal

## Why

`strata api` and `strata worker` write only text logs to stderr. A developer cannot see how long a request takes, why a document waits in the queue, or which step of a job failed. Tech design 0133 (accepted on 2026-11-24, ADR 0012) adds traces, logs and metrics over OpenTelemetry, and a local Grafana that shows them.

## What Changes

- Add the module `crates/strata/src/telemetry.rs`. It builds the `tracing` subscriber of `strata api` and `strata worker`.
  - It always keeps the stderr log of today.
  - When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, it adds three exporters over OTLP/HTTP with protobuf: spans, log events and metrics.
  - The service name is `strata-api` or `strata-worker`. `OTEL_SERVICE_NAME` overrides it.
  - At a clean stop, `main` flushes the exporters.
  - An export error goes to stderr. A request or a job goes on.
- Add the spans of 0133:
  - one span for each API request, with the HTTP attributes, and `strata.tenant_id` and `strata.account_id` after authentication.
  - one child span for each query to PostgreSQL, with the name of the query and no SQL text.
  - one span for each job, one child span for each attempt, and one span for the call of `strata-extract`.
- The API reads no `traceparent` header of a client.
- Add migration V5: a nullable `traceparent` column on `jobs`, with a check constraint on the W3C format. The finish writes the context of its span. The worker makes the span of the job a child of it. A null or invalid value gives a root span.
- Add the seven metrics of 0133. No metric attribute holds a tenant ID, a document ID, an account ID or a revision number.
- No span, log record or metric holds document text, query text, a `sub` or a signed URL. An error event whose text can hold a value of a client records a fixed error class.
- **BREAKING** for the request log: the event `request for an actor` records the `account_id` of the caller, not `act.sub`. It moves after the identity transaction (0021, amended on 2026-11-24).
- Add three services to `compose.yaml` in the profile `observability`: `lgtm` (`grafana/otel-lgtm`), `otel-collector` (`otel/opentelemetry-collector-contrib`) and `pgbouncer-exporter`. Each image is pinned by tag and digest. Each port binds to `127.0.0.1`.
- Add `dev/otel-collector.yaml`: the receivers OTLP, `postgresql` and `prometheus` (Garage and PgBouncer), and one exporter to `lgtm`.
- Add an `[admin]` section to `dev/garage.toml`, so Garage exposes `/metrics`.
- Add the dashboard `dev/grafana/strata.json`, provisioned into Grafana.
- `make dev` always starts the profile and sets the endpoint. When `CI` is set, `scripts/dev.sh` does neither.
- Add `scripts/observability-check.sh`. It queries Tempo, Loki and Prometheus after a run of `strata.hurl`, for the manual check TC-133-07.

Not in this change:

- Production: backends, retention, sampling, alerts, access control and TLS to the collector ([#132](https://github.com/sravioli/strata/issues/132)).
- Audit records of access decisions ([#67](https://github.com/sravioli/strata/issues/67)).
- Telemetry from `strata-extract` itself. It has no network in nsjail.
- A `traceparent` header from a client (#132 decides it for production).

## Decisions for this change

The owner decided these points in #133 and in the design on 2026-11-24:

- Local only for now. Logs and metrics go over OTLP push. The services have no `/metrics` endpoint.
- The trace context crosses the queue in a `traceparent` column. The span of a job is a child, and each attempt is a child span.
- The actor event logs the account ID, not `act.sub`.
- `make dev` is always on, and off in CI.
- TC-133-07 is a manual check with a script. CI does not run it.

The agent decided these points. The owner reviews them in the pull request:

- **`strata.hurl` moves into the repo**, at `dev/strata.hurl`, with a `make hurl` target that sets its variables. The manual check of TC-133-07 runs it.
- **The in-memory exporters of `opentelemetry_sdk`** (its `testing` feature) collect the telemetry in the L1 tests. `strata` gets a `telemetry::for_tests` function behind the feature `testing`, so a test can read what the services export.
- **The names of the queries** for the span names are the names of the constants in `db/` in lower case, for example `WRITE_FINISH` gives `write_finish`.

## Test cases

This change carries these test cases of 0133, each as a scenario with its TC ID:

- TC-133-01 to TC-133-06 (L1).
- TC-133-07 (L2, manual, with `scripts/observability-check.sh`).

It changes this test case of 0021, with its scenario and its test:

- TC-21-13: the request log contains the account ID of the caller and no `act.sub`.

## Capabilities

### New Capabilities

- `observability`: the telemetry of `strata api` and `strata worker`. It covers the exporters and their switch, the spans, the trace across the job queue and the metrics. It also covers the rules on IDs and text, and the local collector and Grafana.

### Modified Capabilities

- `request-authentication`: the requirement "The actor claim goes to the log only" logs the account ID of the caller, not `act.sub` (TC-21-13).
- `job-queue`: the job row has a nullable `traceparent` in the W3C format.
- `dev-services`: the profile `observability` in `compose.yaml`, and `make dev` starts it and sets the endpoint, except in CI.

## Impact

- `crates/strata/src/`: `telemetry.rs`, `main.rs`, the spans in `api.rs`, `upload.rs`, `status.rs`, `search.rs`, `limits.rs`, `worker.rs`, `sandbox.rs`, `extraction.rs` and `db/`, the metrics, and the move of the actor event in `auth/mod.rs`.
- `crates/strata/migrations/V5__job_traceparent.sql`.
- `crates/strata-tests/`: the L1 tests of TC-133-01 to TC-133-06, and the changed test of TC-21-13.
- `compose.yaml`, `dev/otel-collector.yaml`, `dev/garage.toml`, `dev/grafana/`, `scripts/dev.sh`, `scripts/observability-check.sh`, `dev/strata.hurl` and the `Makefile`.
- New dependencies: `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` (HTTP only, no `tonic`), `opentelemetry-appender-tracing` and `tracing-opentelemetry`. `make deny` checks them.
- `core` does not change, so the Lean translation and the proofs do not change.
- No data exists outside local use and CI, so V5 changes no data that matters.
