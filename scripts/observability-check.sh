#!/usr/bin/env bash
# TC-133-07, a manual check (tech design 0133): after `make dev` and `make hurl`, the signals of
# the run reach Grafana. It reads Tempo, Loki and Prometheus through the data source proxy of the
# Grafana of the profile observability, prints what it finds, and exits 1 when a part is missing.
# Then look at the dashboard Strata on http://127.0.0.1:3000.
# Usage: scripts/observability-check.sh [GRAFANA_URL]
set -euo pipefail
grafana=${1:-http://127.0.0.1:3000}
proxy=$grafana/api/datasources/proxy/uid
fail=0

get() { curl -fsS --get "$@"; }
indent() { while IFS= read -r l; do echo "  $l"; done; }
check() { # check NAME COUNT: one line, and a failure for a count of 0.
  if [ "$2" -gt 0 ]; then echo "ok: $1 ($2)"; else echo "missing: $1"; fail=1; fi
}

# Tempo: one trace with the spans of the finish, the job and the extraction.
trace=$(get "$proxy/tempo/api/search" --data-urlencode 'q={ name = "job ProcessDocument" }' \
  --data-urlencode limit=1 | jq -r '.traces[0].traceID // empty')
if [ -n "$trace" ]; then
  names=$(get "$proxy/tempo/api/v2/traces/$trace" \
    | jq -r '[.trace.resourceSpans[].scopeSpans[].spans[].name] | unique | .[]')
  echo "trace $trace:"
  indent <<<"$names"
  for n in "POST /v1/documents/{document_id}/revisions/{revision}/finish" "job ProcessDocument" \
    attempt extraction; do
    check "the span $n" "$(grep -cxF "$n" <<<"$names" || true)"
  done
else
  check "a trace with the span job ProcessDocument" 0
fi

# Loki: the log records of both services.
for s in strata-api strata-worker; do
  n=$(get "$proxy/loki/loki/api/v1/query_range" --data-urlencode "query={service_name=\"$s\"}" \
    --data-urlencode since=1h --data-urlencode limit=100 | jq '[.data.result[].values[]] | length')
  check "the log records of $s" "$n"
done

# Prometheus: http_server_request_duration_seconds for each route of dev/strata.hurl.
routes=$(get "$proxy/prometheus/api/v1/query" \
  --data-urlencode 'query=count by (http_route) (http_server_request_duration_seconds_count)' \
  | jq -r '.data.result[].metric.http_route // empty')
echo "routes with http_server_request_duration_seconds:"
indent <<<"$routes"
for r in /v1/documents "/v1/documents/{document_id}/revisions/{revision}/finish" \
  "/v1/documents/{document_id}/revisions/{revision}" /v1/revisions /v1/search; do
  check "the series of $r" "$(grep -cxF "$r" <<<"$routes" || true)"
done

exit "$fail"
