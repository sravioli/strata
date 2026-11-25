# Tasks

## 1. The telemetry module

- [x] 1.1 Add `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `opentelemetry-appender-tracing` and `tracing-opentelemetry` to `crates/strata`, pinned with `=` like the other dependencies. Find the newest versions that work together (0133 names `opentelemetry` 0.33.1 and `tracing-opentelemetry` 0.34.0). Use `default-features = false` on `opentelemetry-otlp`, with only its OTLP/HTTP protobuf features. Verify: `cargo tree -p strata` shows no `tonic`, and `make deny` passes.
- [x] 1.2 Add `crates/strata/src/telemetry.rs`. It builds the `tracing` subscriber of `strata api` and `strata worker`: the stderr layer of today, filtered by `RUST_LOG`, always. When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, it also adds the exporters of spans, log events and metrics over OTLP/HTTP. The service name is `strata-api` or `strata-worker`, and `OTEL_SERVICE_NAME` overrides it. `main` keeps the guard and flushes the exporters at a clean stop. Behind the feature `testing`, `telemetry::for_tests` gives the in-memory exporters of `opentelemetry_sdk`, so an L1 test can read the spans, the log records and the metrics. Verify: the L1 test TC-133-01 passes. Remove its pending marker.
- [x] 1.3 An export error writes to stderr and changes no answer and no result. The flush at stop has a time limit, so a service stops in less than 5 seconds at SIGTERM when the collector does not answer. Verify: the L1 test TC-133-06 passes. Remove its pending marker.

## 2. The actor event

- [x] 2.1 Move the event `request for an actor` of `auth/mod.rs` after the identity transaction. It records the account ID of the caller, not `act_sub`. Change the test of TC-21-13 in `crates/strata-tests/tests/auth.rs`: the log contains the account ID and does not contain the subject of the actor. Verify: the L1 test TC-21-13 passes.

## 3. The trace context on the job

- [x] 3.1 Add the migration `V5__job_traceparent.sql`: a nullable `traceparent text` column on `jobs`, with a constraint on the W3C format of the spec `job-queue`. Update the L1 test of the columns of `jobs`. Verify: the scenario "The columns of the table" passes, and the grants test passes unchanged.
- [x] 3.2 Add the parse and the format of a `traceparent` value, with L0 tests of a valid value, of each wrong part, and of uppercase hex. `WRITE_FINISH` writes the context of the span of the finish. A `RemoveFiles` job writes null. A retry keeps the value. Verify: the L0 tests pass, and the first half of TC-133-05 (PostgreSQL rejects a wrong value) passes as an L1 test.

## 4. Spans

- [x] 4.1 Add one span for each API request in the router of `api.rs`, with `http.request.method`, `http.route` (the template of the matched route) and `http.response.status_code`. After authentication, add `strata.tenant_id` and `strata.account_id`. The API reads no `traceparent` header of a client. Verify: the L1 test of the scenario "A traceparent header of a client" passes.
- [x] 4.2 Add a child span for each query in `crates/strata/src/db/`, named after its constant in lower case, with `db.system.name = postgresql` and no SQL text and no parameter. The span of a search records the length of `q` and the `limit`, not `q`. Verify: an L1 test finds the span `write_finish` under the span of a finish, with no attribute that holds SQL.
- [x] 4.3 In the worker, add one span for each job, as a child of the `traceparent` of its row. A null or invalid value gives a root span. Add one child span for each attempt, with `strata.attempt`, `strata.job_id` and the result. Add a span for the call of `strata-extract`, with the format, the run time, the exit class of `sandbox.rs` and the number of text parts. Verify: the L1 test TC-133-02 and the second half of TC-133-05 pass. Remove their pending markers.

## 5. Metrics

- [x] 5.1 Add the seven metrics of the spec `observability` with the attribute sets of 0133. The worker reads the count and the age of the oldest `Waiting` job for each kind every 15 seconds. Add L0 tests: each attribute set of each metric has only the keys of 0133. Verify: the L0 tests, the L1 test TC-133-04 and an L1 test of the scenario "No metrics endpoint" pass. Remove its pending marker.

## 6. Content and personal data

- [x] 6.1 Read each `tracing` event and each span attribute of `crates/strata/src`. Some events record the `Display` text of an error that can hold a value of a client. Examples are a database error with a detail, and an error of the HTTP client of the storage with a URL. Each such event records a fixed error class instead. List each changed event in the pull request. Verify: the L1 test TC-133-03 passes, with a token that has an `act` claim. Remove its pending marker.

## 7. The local services

- [x] 7.1 Add three services to `compose.yaml` in the profile `observability`: `lgtm` (`grafana/otel-lgtm`), `otel-collector` (`otel/opentelemetry-collector-contrib`) and `pgbouncer-exporter` (`prometheuscommunity/pgbouncer-exporter`). Pin each image by tag and digest. Bind each port to `127.0.0.1`. Add `dev/otel-collector.yaml`. Its receivers are OTLP/HTTP, `postgresql` and `prometheus` (Garage and the PgBouncer exporter). Its one exporter goes to `lgtm`. Add the `[admin]` section with a development token to `dev/garage.toml`. Find out whether the Grafana of the pinned `otel-lgtm` lets a local user in with no password. Write the result in the pull request. Verify: the scenarios "The default start leaves the profile out" and "The profile starts and listens on localhost" pass locally. Run them with Podman, and with Docker where it is installed.
- [x] 7.2 Add the dashboard `dev/grafana/strata.json`, provisioned into Grafana: the request rate, the errors and the latency for each route, the rejections, the queue and the extractions. Verify: the dashboard loads in Grafana after `make dev`, and each panel shows data after a run of `dev/strata.hurl`.
- [x] 7.3 In `scripts/dev.sh`, when `CI` is not set, start the profile `observability` and set `OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318` for `strata api` and `strata worker`. When `CI` is set, do neither. `--stop` stops no container. Verify: the scenarios "A local run" and "A run in CI" pass. The second one passes as `CI=1 make l2` locally, and in the `l2` job of CI.
- [ ] 7.4 Move `strata.hurl` into the repo at `dev/strata.hurl`. Add the target `make hurl`. It sets `base_url`, the tokens of `acme` and `globex` and the size of the fixture, and runs `hurl --test`. Add `scripts/observability-check.sh` with the ID TC-133-07. It queries the APIs of Tempo, Loki and Prometheus for the trace, the log records and the series of the spec. It prints what it finds. Document both in `dev/README.md` (or the existing developer docs). Verify: after `make dev` and `make hurl`, the script finds the trace, the log records and the series. The owner looks at the dashboard (TC-133-07). Remove its pending marker.

## 8. Integration and gates

- [ ] 8.1 Run `make`, `make l1`, `CI=1 make l2`, `make l2` and `make deny`. Verify: each target passes, the coverage of `CONSTRAINTS.md` is at the floor or above, and `python3 scripts/check-test-cases.py --report` shows TC-133-01 to TC-133-07 and TC-21-13 as `implemented`. If the coverage is below the floor, add tests. Do not lower the number.
- [ ] 8.2 Comment on #133 with the result of the manual run of TC-133-07 and the list of events that task 6.1 changed. Add what task 7.1 found about the Grafana login. Verify: the comment exists, and #132 has each point that this change leaves to production.

## Workflow follow-up

- After the owner merges the pull request, archive the change with `openspec-archive-change`, and close #133.
