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
- `lgtm` is Grafana with Tempo, Loki and Prometheus, on `http://127.0.0.1:3000`. The dashboard `Strata` (`grafana/strata.json`) shows the requests, the rejections, the queue and the extractions, and in the row Database, the connections and transactions of PostgreSQL and the pools of PgBouncer.
- `pgbouncer-exporter` gives the statistics of PgBouncer to the collector.

`strata api` and `strata worker` export their traces, logs and metrics to the collector. `make dev` sets `OTEL_METRIC_EXPORT_INTERVAL=10000`, so the metrics go out each 10 seconds, not each 60 seconds. The collector keeps the data in memory, so a restart of the services loses it.

The Grafana of the pinned `grafana/otel-lgtm` image lets each local user in with no password, with the role Admin. Grafana listens only on `127.0.0.1`.

The manual check TC-133-07: after `make dev` and `make hurl`, wait one minute, then run `scripts/observability-check.sh`. It reads Tempo, Loki and Prometheus through Grafana and prints the trace, the log records and the series that it finds. Then look at the dashboard. CI does not run this check.

## The frontend for demonstrations

`make web` runs the site of `web/` (tech design 0065) on `http://localhost:4321`. It needs Node and npm, and the processes of `make dev`. When the variable `CI` is not set, `make dev` starts `strata api` with `STRATA_CORS_ORIGINS=http://localhost:4321`. `make web` stops with a message when the API does not answer, or when it does not allow that origin.

`make web` starts Keycloak, the compose profile `frontend`, on `http://localhost:8180`. Then it registers the issuer `http://localhost:8180/realms/strata` (algorithm `RS256`, audience `strata`, tenant claim `tenant`) and binds the tenants `acme` and `globex` to it. It applies the CORS rule of `garage-cors.xml` to the bucket, and runs the Astro dev server. Ctrl-C stops the dev server. Keycloak runs on.

Sign in with one of the seed users of `keycloak/strata-realm.json`:

| User | Tenant | Password |
|---|---|---|
| `alice@acme` | `acme` | `alice-acme-dev` |
| `bob@acme` | `acme` | `bob-acme-dev` |
| `alice@globex` | `globex` | `alice-globex-dev` |

The admin console of Keycloak is on `http://localhost:8180/admin`, as `admin` with the password `admin`.

Keycloak keeps no data. A new container imports `keycloak/strata-realm.json` again. After a change to that file, run:

```sh
podman compose --profile frontend up -d --force-recreate keycloak
```

A new container has a new signing key. The API reads the keys of the issuer again for a key ID that it does not know, at most once each 60 seconds. So the first sign-in after a new container can fail for up to one minute.

The site keeps the token in `sessionStorage`, and the uploads of each person in `localStorage`. The API has no route that lists the documents of a tenant yet (#140).

The browser test TC-65-07: after `make dev` and `make web`, run `npx playwright install chromium` one time in `web/`, then `npm run test:e2e`. CI does not run it (#141).
