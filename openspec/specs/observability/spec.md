# observability Specification

## Purpose
Makes the behaviour of `strata api` and `strata worker` visible as OpenTelemetry traces, logs and metrics, and shows them in a local Grafana (tech design 0133, ADR 0012). Production is #132.

## Requirements

### Requirement: The telemetry is off until an endpoint is set

`strata api` and `strata worker` SHALL export traces, logs and metrics over OTLP/HTTP only when `OTEL_EXPORTER_OTLP_ENDPOINT` is set. With the variable unset, a service SHALL export nothing and open no connection for telemetry. In both cases, the service SHALL keep its log on stderr, filtered by `RUST_LOG`. The service name SHALL be `strata-api` or `strata-worker`, and `OTEL_SERVICE_NAME` SHALL override it. At a clean stop, a service SHALL send the telemetry that it holds.

#### Scenario: TC-133-01 No endpoint, no telemetry

- **GIVEN** `OTEL_EXPORTER_OTLP_ENDPOINT` is not set
- **WHEN** `strata api` and `strata worker` start and serve a request and a job
- **THEN** they open no connection for telemetry
- **AND** the stderr log is as before

### Requirement: The telemetry never breaks a request

An export that fails SHALL write an error to stderr and SHALL not change the answer of a request or the result of a job. A service SHALL stop in less than 5 seconds at SIGTERM, also when the collector does not answer.

#### Scenario: TC-133-06 The collector does not listen

- **GIVEN** `OTEL_EXPORTER_OTLP_ENDPOINT` points to a port where nothing listens
- **WHEN** an account uploads a document and searches
- **THEN** each request gets the same answer as with telemetry off
- **AND** the job ends `Done`
- **AND** at SIGTERM, each service stops in less than 5 seconds

### Requirement: The API makes one span for each request

The API SHALL make one span for each request, with `http.request.method`, `http.route` and `http.response.status_code`. `http.route` SHALL be the template of the route. After authentication, the span SHALL also have `strata.tenant_id` and `strata.account_id`. Each query to PostgreSQL SHALL have a child span with the name of the query, and with no SQL text and no parameter. The API SHALL not read a `traceparent` header of a client.

#### Scenario: A traceparent header of a client

- **GIVEN** the exporters write to memory
- **WHEN** a client sends a request with a valid `traceparent` header
- **THEN** the span of the request has a new trace ID, not the trace ID of the header

### Requirement: One trace follows a document through the queue

The finish SHALL write the trace context of its span on the job that it queues. The worker SHALL make one span for each job, as a child of that context, and one child span for each attempt. A job with no context SHALL get a root span. The call of `strata-extract` SHALL have its own span. That span SHALL have the format, the run time, the exit class and the number of text parts.

#### Scenario: TC-133-02 The finish and its job are one trace

- **GIVEN** the exporters write to memory
- **WHEN** an account finishes an upload and the worker runs the job
- **THEN** the job row has a `traceparent` with the trace ID of the finish request
- **AND** the span of the job, its attempt and the extraction have the same trace ID
- **AND** the span of the job is a child of the span of the finish

### Requirement: The API records its metrics

The API SHALL record `http.server.request.duration`, by method, route template and status code. It SHALL record `strata.api.rejections`, by the reason `bound`, `rate_limit` or `search_timeout`. It SHALL record `strata.api.requests_in_flight`. The services SHALL push the metrics over OTLP, and SHALL have no `/metrics` endpoint.

#### Scenario: No metrics endpoint

- **GIVEN** `make dev`, run locally
- **WHEN** a client sends `GET /metrics` to the API
- **THEN** the API answers `404`

### Requirement: The worker records its metrics

The worker SHALL record `strata.jobs.waiting` and `strata.jobs.oldest_waiting_age`, by kind. It SHALL read them every 15 seconds. It SHALL record `strata.job.attempts`, by kind and result, and `strata.extraction.duration`, by format and result.

#### Scenario: TC-133-04 No ID in a metric

- **GIVEN** the exporters write to memory
- **WHEN** two tenants each send requests to each route and each run a job
- **THEN** no metric data point has an attribute that holds a tenant ID, a document ID, an account ID or a revision number

### Requirement: No content and no personal data in the telemetry

No span attribute, log record or metric attribute SHALL hold document text or the query text of a search. None SHALL hold a token, its `sub` or its `act.sub`, or a signed URL. The span of a search SHALL record the length of `q` and the `limit`. Some log events record the text of an error. If that text can hold a value of a client, the event SHALL record a fixed error class instead.

#### Scenario: TC-133-03 The word, the subject and the URL stay out

- **GIVEN** the exporters write to memory, and a PDF with a unique word
- **WHEN** an account uploads the PDF, the worker extracts it, and the account searches for the word
- **AND** the account sends a request with a token that has an `act` claim
- **THEN** no span attribute, no log record and no metric attribute holds the word, the `sub` or `act.sub` of the token, or the signed upload URL

### Requirement: Local Grafana shows the three signals

The local services SHALL include a collector that receives OTLP/HTTP on `127.0.0.1:4318`, and Grafana with Tempo, Loki and Prometheus on `127.0.0.1:3000`. The collector SHALL also read the metrics of PostgreSQL, PgBouncer and Garage. Grafana SHALL have one provisioned dashboard: the request rate, the errors and the latency for each route, the rejections, the queue, the extractions, PostgreSQL and PgBouncer. `scripts/observability-check.sh` SHALL query and print the three signals.

#### Scenario: TC-133-07 The signals of a run reach Grafana

- **GIVEN** `make dev`, run locally by a developer. CI does not run this check (decided by the owner on 2026-11-24)
- **WHEN** `strata.hurl` runs, and then `scripts/observability-check.sh`
- **THEN** Tempo has one trace with the spans of the finish, the job and the extraction
- **AND** Loki has the log records of both services
- **AND** Prometheus has the series `http_server_request_duration_seconds` for each route of the run
