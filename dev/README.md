# Local development

The files in this folder configure the services of `compose.yaml` for local use and for the L1 tests. Each password, key and token here is for development only.

## Run Strata locally

`make dev` starts the services, applies the migrations and runs `strata api` on `127.0.0.1:8080` and `strata worker`. It also runs `strata-dev`, the development issuer, and creates the tenants `acme` and `globex`. A token:

```sh
target/debug/strata-dev token --tenant acme --sub alice
```

`make hurl` runs `dev/strata.hurl` against `make dev`. It uploads the fixture `two-pages.pdf`, waits until the revision is ready, reads the status and searches as `acme` and as `globex`. It needs [Hurl](https://hurl.dev). `yaak/` holds the same requests for Yaak.

## Telemetry

When the variable `CI` is not set, `make dev` also starts the compose profile `observability` (tech design 0133):

- `otel-collector` receives OTLP/HTTP on `127.0.0.1:4318`, and reads the metrics of PostgreSQL, PgBouncer and Garage. Its configuration is `otel-collector.yaml`.
- `lgtm` is Grafana with Tempo, Loki and Prometheus, on `http://127.0.0.1:3000`. The dashboard `Strata` (`grafana/strata.json`) shows the requests, the rejections, the queue and the extractions.
- `pgbouncer-exporter` gives the statistics of PgBouncer to the collector.

`strata api` and `strata worker` export their traces, logs and metrics to the collector. The metrics go out each 60 seconds. The collector keeps the data in memory, so a restart of the services loses it.

The Grafana of the pinned `grafana/otel-lgtm` image lets each local user in with no password, with the role Admin. Grafana listens only on `127.0.0.1`.

The manual check TC-133-07: after `make dev` and `make hurl`, wait two minutes, then run `scripts/observability-check.sh`. It reads Tempo, Loki and Prometheus through Grafana and prints the trace, the log records and the series that it finds. Then look at the dashboard. CI does not run this check.
