- Issue: [#133](https://github.com/sravioli/strata/issues/133)
- Status: accepted
- Start date: 2026-11-24
- Decided by: sravioli, 2026-11-24

# Observability, local: OpenTelemetry and Grafana

## Problem description

Today, `strata api` and `strata worker` write text logs to stderr, and nothing else. A developer cannot see how long a request takes, why a document waits, or which step of a job failed. This design adds traces, logs and metrics over OpenTelemetry, and a local Grafana that shows them. One trace follows a document from the finish of the upload, through the job queue, to the end of the extraction.

Goals:

- `strata api` and `strata worker` export traces, logs and metrics over OTLP when `OTEL_EXPORTER_OTLP_ENDPOINT` is set.
- One trace holds the API request of the finish, the wait in the queue, each attempt of the job and the extraction.
- One command starts a local collector and Grafana. The collector also gets the metrics of PostgreSQL, PgBouncer and Garage.

Non-goals:

- Production. The backends, retention, sampling, alerts and access control of a deployed system are in [#132](https://github.com/sravioli/strata/issues/132).
- Audit records of access decisions ([#67](https://github.com/sravioli/strata/issues/67)).
- Telemetry from `strata-extract`. It runs in nsjail with no network (0007).
- A change to the L1 or L2 tests of CI, apart from the new test cases of this design.

## Background and references

- The requirements ask for "OpenTelemetry traces, structured logs, metrics, alerts and audit logging" (`docs/reference/handoff.md`, "Observability and platform"). They also forbid tenant IDs in metric labels: "Do not put 30,000 tenant IDs into high-cardinality metric labels."
- Tech design 0049 says that a search logs "no text of the query". This design keeps that rule for each signal.
- The services use `tracing` 0.1 and `tracing-subscriber` 0.3 with an `EnvFilter` (`crates/strata/src/main.rs`). Each log event is a `tracing` event.
- The API queues a job in the transaction of the finish (`WRITE_FINISH` in `crates/strata/src/db/upload.rs`, tech design 0005). `strata_api` has `insert` on all columns of `jobs`.
- OpenTelemetry: the OTLP protocol, the W3C Trace Context `traceparent` header, the semantic conventions for HTTP spans and metrics, and the SDK environment variables (`OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME`, `OTEL_SDK_DISABLED`).
- The Rust crates, current on 2026-11-24: `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` and `opentelemetry-appender-tracing` 0.33.1, and `tracing-opentelemetry` 0.34.0.
- `grafana/otel-lgtm` 0.41.1 (2026-11-20): one image with Grafana, Tempo (traces), Loki (logs), Prometheus (metrics) and an OpenTelemetry Collector.

## Requirements

The owner decided four points in #133 on 2026-11-24:

1. Local only for now. Production follows in #132.
2. Logs go over OTLP.
3. Metrics go over OTLP push to a collector. The services have no `/metrics` endpoint.
4. The trace context crosses the job queue as a nullable W3C `traceparent` column on the job row. The span of the job is a child of that context, and each attempt is a child span.

Further constraints:

- **Off by default.** With `OTEL_EXPORTER_OTLP_ENDPOINT` unset, a service exports nothing and opens no connection. The stderr log stays as it is. So `make l1` and CI do not change. `make dev` sets the variable locally (see "Local services").
- **No tenant ID in a metric attribute.** Tenant IDs, document IDs, account IDs and revision numbers go only in span attributes and log fields. A metric attribute has a small, fixed set of values: a route template, a method, a status code, a job kind, a result class.
- **No document text and no query text** in a span, a log record or a metric. The search route records the length of `q` and the `limit`, not `q`.
- **No personal data.** A span has the `account_id` (a UUID of Strata), not the `sub` of the token. Tech design 0021 wrote `act.sub` in the request log (TC-21-13). This design amends 0021: the event writes the `account_id` of the caller instead (decided by the owner, 2026-11-24).
- **The telemetry never breaks a request.** An export error goes to stderr. The request or the job goes on.
- **The volume is small.** A developer sends some requests each minute. The collector keeps the data in memory, and a restart loses it.

## Proposed design

```text
strata api ─┐ OTLP/HTTP :4318        ┌─ Tempo (traces)
            ├────────────► collector ┼─ Loki (logs)      ─► Grafana :3000
strata worker┘               ▲  ▲  ▲  └─ Prometheus (metrics)
              PostgreSQL ────┘  │  └──── Garage /metrics
              (postgresql receiver)  PgBouncer exporter /metrics
```

**One telemetry module.** A new module `crates/strata/src/telemetry.rs` builds the `tracing` subscriber of each binary. It always adds the stderr layer of today. When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, it also adds three exporters over OTLP/HTTP with protobuf. A `tracing-opentelemetry` layer exports the spans, an `opentelemetry-appender-tracing` layer the log events, and a meter provider the metrics. The service name is `strata-api` or `strata-worker`, and `OTEL_SERVICE_NAME` overrides it. `main` keeps the guard of the module and flushes the exporters at a clean stop. OTLP/HTTP needs no gRPC stack. With `default-features = false` on `opentelemetry-otlp` and only its HTTP features, the binaries do not get `tonic`.

**Spans.**

- The API has one span for each request. The span gets the attributes of the HTTP conventions: `http.request.method`, `http.route` (the template of the route, for example `/v1/documents/{document_id}/revisions/{revision}`) and `http.response.status_code`. After authentication, it also gets `strata.tenant_id` and `strata.account_id`.
- The API does not read a `traceparent` header of a client. Each request starts a new trace, because a client value is untrusted input. #132 decides this for production.
- Each query to PostgreSQL has a child span. The name of the span is the name of the query (for example `write_finish`), with `db.system.name = postgresql`. The span has no SQL text and no parameter.
- The worker has one span for each job, `job ProcessDocument` or `job RemoveFiles`. Its parent is the `traceparent` of the job row, if the row has one. Each attempt is a child span, with `strata.attempt` (the attempt number), `strata.job_id` and the result.
- The call of `strata-extract` has its own span. It records the format, the run time, the exit class of `sandbox.rs` and the number of text parts. It records no text.

**Log events.** The OTLP log layer exports each `tracing` event at the level of `RUST_LOG`, with the trace ID and the span ID of its span. The event `request for an actor` of `auth/mod.rs` moves after the identity transaction and records `account_id`, not `act_sub` (amended 0021). The implementation also changes the scenario TC-21-13 of the spec `request-authentication` and its test in `auth.rs`. Some events record the `Display` text of an error, for example of `tokio_postgres` or of the HTTP client of the storage. The implementation checks each such event. An event whose text can hold a value of a client gets a fixed error class instead of the text.

**The trace across the queue.** Migration V5 adds `traceparent text` to `jobs`. The column is null, or it holds a value of the W3C format. That is version `00`, a trace ID of 32 lowercase hex digits, a span ID of 16 and flags of 2. A check constraint enforces the format. `WRITE_FINISH` writes the context of the span of the finish request, so the job and the request belong to the same trace. A job that the API queues with no active span, and each `RemoveFiles` job, has null. The worker reads the column at the claim. A null or invalid value gives a new root span and no error. A retry of a job reuses the same `traceparent`, so all attempts are in one trace.

**Metrics.** The names follow the semantic conventions where one exists. Otherwise they have the prefix `strata.`.

| Metric | Type | Attributes | Source |
|---|---|---|---|
| `http.server.request.duration` | histogram, s | `http.request.method`, `http.route`, `http.response.status_code` | API |
| `strata.api.rejections` | counter | `reason`: `bound`, `rate_limit`, `search_timeout` | API |
| `strata.api.requests_in_flight` | up-down counter | none | API, against the bound of 256 |
| `strata.jobs.waiting` | gauge | `kind` | worker, one query every 15 s |
| `strata.jobs.oldest_waiting_age` | gauge, s | `kind` | worker, the same query |
| `strata.job.attempts` | counter | `kind`, `result` | worker |
| `strata.extraction.duration` | histogram, s | `format`, `result` | worker |

`result` has the values of the code: the job results (for example `Done`, `Parked`) and the exit classes of `sandbox.rs`. No attribute holds an ID.

**Local services.** `compose.yaml` gets three services in the compose profile `observability`. So `docker compose up` and `make l1` do not start them.

- `lgtm`: `grafana/otel-lgtm`, pinned by tag and digest. Grafana listens on `127.0.0.1:3000`.
- `otel-collector`: `otel/opentelemetry-collector-contrib`, pinned. It listens on `127.0.0.1:4318` for OTLP/HTTP. Its configuration is `dev/otel-collector.yaml`. Its receivers are OTLP, `postgresql` (as the `strata` user of compose), and a `prometheus` receiver that reads Garage and the PgBouncer exporter. It sends everything to `lgtm`.
- `pgbouncer-exporter`: `prometheuscommunity/pgbouncer-exporter`, pinned.

Garage exposes `/metrics` on its admin port when `dev/garage.toml` has an `[admin]` section. The design adds that section with a development token.

`make dev` always starts the profile and sets `OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318` for `strata api` and `strata worker` (decided by the owner, 2026-11-24). When the variable `CI` is set, as in each GitHub Actions job, `scripts/dev.sh` starts neither and sets no endpoint. So `make l2` in CI runs with telemetry off, and a local `make l2` runs with it on. `dev/grafana/strata.json` is one dashboard, provisioned into Grafana: the request rate, the errors and the latency for each route, the rejections, the queue and the extractions.

## Alternatives considered

**`grafana/otel-lgtm` alone, with its own collector.** It is one container. But the collector of the image has a fixed configuration. The metrics of PostgreSQL, PgBouncer and Garage would need a changed copy of a file inside the image. A separate collector keeps our configuration in one file of the repo. Production also needs a collector of its own (#132), so this shape stays when the backends change.

**A Prometheus `/metrics` endpoint in each service.** Prometheus would scrape it, and a failed scrape would show a stopped service. The owner chose OTLP push (decision 3): one exporter for the three signals, and no new port in the services. A stopped service shows as the end of its data, and `make dev` shows its processes.

**A span link instead of a parent for the job.** The OpenTelemetry conventions for messaging prefer a link when a consumer reads a message later. Then the job is a separate trace. A parent is easier to follow locally, and the owner chose it (decision 4). A change to a link later changes only the code of the worker. The column stays the same.

**The trace context in a separate table.** It would leave `jobs` unchanged. But each claim would need a join, and the two rows could disagree. One nullable column is smaller.

## Proof obligations

N/A. This design adds no function to `crates/core`, and no domain rule. L0 tests cover the parse of `traceparent` and the attribute sets of the metrics. L1 tests cover the export, the trace across the queue and the rules on tenant IDs and text (see Test cases).

## Impacts

### Infrastructure

- New local services only, in the compose profile `observability`. No deployed infrastructure, and no change to CI.
- Their ports bind to `127.0.0.1`: 3000 (Grafana) and 4318 (OTLP/HTTP).
- The collector keeps no data on disk. A restart of `lgtm` loses the telemetry.
- To roll back: remove the profile and the module. The `traceparent` column can stay, because it is nullable.

### Security and tenant isolation

- The telemetry holds tenant IDs, document IDs and account IDs. It holds no document text, no query text, no `sub`, no token and no signed URL. The signed upload URL is in no span, because the span of a start records the route template, not the response body.
- The documentation of `otel-lgtm` says that its Grafana lets each local user in with no password. The implementation checks this on the pinned image. Its port binds to `127.0.0.1`. Access to the telemetry of a deployed system is a point of #132.
- The API reads no `traceparent` of a client, so a client cannot add spans to a trace of Strata.
- The worker reads `traceparent` from its own database. The check constraint and the parse limit it to the W3C format. A bad value gives a new root span, and the job runs as before.
- The `postgresql` receiver of the collector uses the development user `strata` of compose. This user exists only locally.
- New dependencies: the OpenTelemetry crates above. `cargo deny` checks their licences and advisories in the pull request of the implementation.

### Data and migration

- V5: `alter table jobs add column traceparent text` with a check constraint on the format. The column is nullable, so the migration does not rewrite the rows. Old rows keep null.
- The grants do not change: `strata_api` has `insert` on all columns of `jobs`, and `strata_worker` has `select`. The L1 test of the grants reads the table ACLs and the explicit column ACLs (`relacl`, `attacl`). A new column adds no entry to either, so the test does not change.
- Old code ignores the column, and new code accepts null. So old and new workers run side by side.

### Developer

- `make dev` starts about 1 GB more of containers. Then Grafana is at `http://127.0.0.1:3000`.
- Each binary gets the OpenTelemetry crates, so a clean build takes longer.
- `RUST_LOG` still filters the stderr log and the exported log records.

## Test cases

L0 tests cover the parse and the format of `traceparent`, and the attribute sets of each metric. Each test case below is L1, except TC-133-07. TC-133-07 is a manual check, with a script: `scripts/observability-check.sh` queries the APIs of Tempo, Loki and Prometheus and prints what it finds. A developer runs it by hand after `make dev`, and looks at the dashboard. CI does not run it (decided by the owner, 2026-11-24). In the L1 tests, the exporters of the SDK write to memory (the in-memory exporters of `opentelemetry_sdk`), so they need no collector.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-133-01 | L1 | Given `OTEL_EXPORTER_OTLP_ENDPOINT` is not set. When `strata api` and `strata worker` start and serve a request and a job. Then they open no connection for telemetry, and the stderr log is as before. | sravioli, 2026-11-24 | pending |
| TC-133-02 | L1 | Given the exporters write to memory. When an account finishes an upload and the worker runs the job. Then the job row has a `traceparent` with the trace ID of the finish request. The span of the job, its attempt and the extraction have the same trace ID. The span of the job is a child of the span of the finish. | sravioli, 2026-11-24 | pending |
| TC-133-03 | L1 | Given the exporters write to memory, and a PDF with a unique word. When an account uploads the PDF, the worker extracts it, and the account searches for the word, also with a token that has an `act` claim. Then no span attribute, no log record and no metric attribute holds the word, the `sub` or `act.sub` of the token, or the signed upload URL. | sravioli, 2026-11-24 | pending |
| TC-133-04 | L1 | Given the exporters write to memory. When two tenants each send requests to each route and each run a job. Then no metric data point has an attribute that holds a tenant ID, a document ID, an account ID or a revision number. | sravioli, 2026-11-24 | pending |
| TC-133-05 | L1 | Given a job row with a `traceparent` that is not in the W3C format. When the row is written. Then PostgreSQL rejects it. Given a job row with null. When the worker claims it. Then the job runs, and its span is a root span. | sravioli, 2026-11-24 | pending |
| TC-133-06 | L1 | Given `OTEL_EXPORTER_OTLP_ENDPOINT` points to a port where nothing listens. When an account uploads a document and searches. Then each request gets the same answer as with telemetry off, and the job ends `Done`. At SIGTERM, each service stops in less than 5 s. | sravioli, 2026-11-24 | pending |
| TC-133-07 | L2 | Given `make dev`, run locally. When `strata.hurl` runs. Then Tempo has one trace with the spans of the finish, the job and the extraction. Loki has the log records of both services. Prometheus has the series `http_server_request_duration_seconds` for each route of the run. | sravioli, 2026-11-24 | pending |

## Unresolved questions

The owner answered the questions of the draft on 2026-11-24. The log has the account ID instead of `act.sub`. `make dev` is always on, and off in CI. TC-133-07 is a manual check with a script.

During implementation:

- The digests of the three images, and the version of the collector.
- Whether `tracing-opentelemetry` 0.34 works with `opentelemetry` 0.33.1, or which pair of versions does.
- The configuration of the `postgresql` and `prometheus` receivers, and the token of the Garage admin port.
