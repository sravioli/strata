---
status: "accepted"
date: 2026-11-24
decision-makers: sravioli
tech-design: docs/tech-design/0133-local-observability.md
---

# Strata exports traces, logs and metrics over OTLP, and a local Grafana shows them

## Context and Problem Statement

`strata api` and `strata worker` write only text logs to stderr. A developer cannot see how long a request takes, why a document waits in the queue, or which step of a job failed. How do the services make their behaviour visible, locally first? The issue is [#133](https://github.com/sravioli/strata/issues/133). Production is [#132](https://github.com/sravioli/strata/issues/132).

## Considered Options

* OpenTelemetry for the three signals over OTLP push, a separate collector, and `grafana/otel-lgtm`
* `grafana/otel-lgtm` alone, with its own collector
* A Prometheus `/metrics` endpoint in each service for the metrics
* A span link instead of a parent for the span of a job

## Decision Outcome

Chosen option: "OpenTelemetry for the three signals over OTLP push, a separate collector, and `grafana/otel-lgtm`". One exporter carries traces, logs and metrics. Our collector configuration is one file of the repo, and the same shape can serve production.

The decision has these parts:

* A service exports only when `OTEL_EXPORTER_OTLP_ENDPOINT` is set. The stderr log stays.
* `make dev` always starts the compose profile `observability` and sets the endpoint. When `CI` is set, it does neither.
* Migration V5 adds a nullable W3C `traceparent` column to `jobs`. The span of a job is a child of the span of the finish request, and each attempt is a child span.
* No metric attribute holds a tenant ID, a document ID, an account ID or a revision number.
* No signal holds document text, query text, a `sub` or a signed URL.
* The actor event of tech design 0021 logs the account ID of the caller, not `act.sub` (0021 amended on 2026-11-24).
* The API reads no `traceparent` header of a client.

### Consequences

* Good, because one trace shows a document from the finish of its upload to the end of its extraction.
* Good, because CI and the L1 tests do not change: the telemetry is off there.
* Good, because a change of backend for production changes only the configuration of the collector.
* Bad, because `make dev` starts about 1 GB more of containers.
* Bad, because the request log no longer names the actor of a token with an `act` claim.
* Bad, because production needs its own design ([#132](https://github.com/sravioli/strata/issues/132)).
