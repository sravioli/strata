#!/usr/bin/env bash
# make dev: the walking skeleton locally (tech design 0049, spec dev-services).
# It starts the services, applies the migrations, starts strata-dev, creates the tenants acme and
# globex with their bindings, and runs strata api and strata worker on the host. When CI is not
# set, it also starts the profile observability, and the API and the worker export to it.
#   scripts/dev.sh               run in the foreground. Ctrl-C stops each process that it started.
#   scripts/dev.sh --background  return when the API answers. The logs go to .strata-dev/*.log.
#   scripts/dev.sh --stop        stop the processes of an earlier run with --background.
# Needs Linux, the services of compose.yaml, and nsjail in .tools/bin (make dev builds it).
set -euo pipefail
cd "$(dirname "$0")/.."

state=.strata-dev
pids=$state/dev.pids
mkdir -p "$state"

stop() {
  [ -f "$pids" ] || return 0
  while read -r pid; do kill "$pid" 2>/dev/null || true; done <"$pids"
  while read -r pid; do
    for _ in $(seq 50); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill -9 "$pid" 2>/dev/null || true
  done <"$pids"
  rm -f "$pids"
}

mode=${1:-}
case "$mode" in
  --stop) stop; exit 0 ;;
  --background | "") ;;
  *) echo "usage: scripts/dev.sh [--background|--stop]" >&2; exit 2 ;;
esac
# The processes of an earlier run hold the ports.
stop

# The development values of compose.yaml and dev/roles.sql.
export AWS_ENDPOINT=http://localhost:3900 AWS_ALLOW_HTTP=true AWS_BUCKET_NAME=strata \
  AWS_DEFAULT_REGION=garage AWS_ACCESS_KEY_ID=GK000000000000000000000000 \
  AWS_SECRET_ACCESS_KEY=0000000000000000000000000000000000000000000000000000000000000000
db() { echo "postgres://$1:$1@localhost:$2/strata"; }
bin=target/debug

# The telemetry is on locally and off in CI (tech design 0133): the profile observability starts,
# and strata api and strata worker export to its collector.
profile=()
if [ -z "${CI:-}" ]; then
  profile=(--profile observability)
  export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318
  # Each 10 s, not the 60 s of the SDK, so the dashboard shows a run soon after it ends.
  export OTEL_METRIC_EXPORT_INTERVAL=10000
else
  unset OTEL_EXPORTER_OTLP_ENDPOINT OTEL_METRIC_EXPORT_INTERVAL
fi
# podman-compose stays attached on a first start without -d.
${COMPOSE:-docker compose} "${profile[@]}" up -d --wait
cargo build --locked -p strata -p strata-dev -p strata-extract
STRATA_DATABASE_URL=$(db strata_owner 5432) $bin/strata migrate

# A process that this script starts. Its output goes to the terminal, or to a log with --background.
start() {
  local name=$1
  shift
  if [ "$mode" = --background ]; then
    "$@" >"$state/$name.log" 2>&1 &
  else
    "$@" &
  fi
  echo $! >>"$pids"
}
# A failed step stops the processes. With --background, they run on after a start that works.
ok=
trap '[ -n "$mode" ] && [ -n "$ok" ] || stop' EXIT
trap 'exit 130' INT TERM

start strata-dev $bin/strata-dev serve --port 8090
admin() { STRATA_DATABASE_URL=$(db strata_operator 5432) $bin/strata admin "$@"; }
# A second run skips each tenant, issuer and binding that exists.
for t in acme globex; do
  admin tenant list | cut -f1 | grep -x "$t" >/dev/null || admin tenant create "$t" >/dev/null
done
issuer=(--jwks-url http://127.0.0.1:8090/jwks.json --audience strata --algorithm EdDSA
  --tenant-claim tenant)
admin issuer register --issuer strata-dev "${issuer[@]}" 2>/dev/null \
  || admin issuer set strata-dev "${issuer[@]}"
for t in acme globex; do
  admin binding remove --issuer strata-dev --tenant-value "$t" 2>/dev/null || true
  admin binding add --issuer strata-dev --tenant-value "$t" --tenant "$t"
done

start api env STRATA_DATABASE_URL="$(db strata_api 6432)" $bin/strata api --listen 127.0.0.1:8080
# The jails of the worker need a delegated cgroup (scripts/in-cgroup.sh).
start worker env STRATA_DATABASE_URL="$(db strata_worker 6432)" \
  STRATA_NSJAIL="$PWD/.tools/bin/nsjail" STRATA_EXTRACT="$PWD/$bin/strata-extract" \
  scripts/in-cgroup.sh $bin/strata worker

# Any HTTP answer means that the API listens.
for _ in $(seq 100); do
  curl -s -o /dev/null http://127.0.0.1:8080/v1/search && break
  sleep 0.1
done
curl -s -o /dev/null http://127.0.0.1:8080/v1/search || { echo "error: the API does not answer" >&2; exit 1; }
ok=1
echo "ok: the API answers on 127.0.0.1:8080. A token: $bin/strata-dev token --tenant acme --sub alice"
[ -n "${CI:-}" ] || echo "ok: Grafana on http://127.0.0.1:3000 (dashboard Strata)"

if [ -z "$mode" ]; then wait; fi
