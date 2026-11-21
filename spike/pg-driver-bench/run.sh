#!/usr/bin/env bash
# Runs each shape x library x path. Usage: ./run.sh <runs> <secs> [paths...]
set -euo pipefail
runs=${1:-5}; secs=${2:-10}; shift 2 || true
paths=("${@:-direct pgbouncer latency1ms}")
read -ra paths <<<"${paths[*]}"
bin=target/release/pg-driver-bench
# The password of the two bench roles. Not in the repository: export BENCH_PASSWORD before the run.
pw=${BENCH_PASSWORD:?export BENCH_PASSWORD first}
admin=postgres://bench_owner:${pw}@127.0.0.1:55432/bench
declare -A url=(
  [direct]=postgres://bench_app:${pw}@127.0.0.1:55432/bench
  [pgbouncer]=postgres://bench_app:${pw}@127.0.0.1:56432/bench
  [latency1ms]=postgres://bench_app:${pw}@127.0.0.1:57432/bench
)
libs=(sqlx tp tp-pipe)
cleanup() {
  podman exec strata-bench_postgres_1 psql -q -U bench_owner -d bench -c \
    "delete from text_parts p using revisions r where r.file_name = 'bench.pdf' and (p.tenant_id, p.document_id) = (r.tenant_id, r.document_id);
     delete from revisions where file_name = 'bench.pdf';
     delete from documents d where not exists (select from revisions r where (r.tenant_id, r.document_id) = (d.tenant_id, d.document_id));" \
    -c "vacuum analyze text_parts, revisions, documents"
}
for run in $(seq "$runs"); do
  for path in "${paths[@]}"; do
    for shape in read status search insert; do
      # Rotate the order of the libraries on each run.
      for i in 0 1 2; do
        lib=${libs[$(( (i + run) % 3 ))]}
        "$bin" "$lib" "$shape" "${url[$path]}" "$admin" "$secs" "$path" | sed "s/^{/{\"run\":$run,/"
        [[ $shape == insert ]] && cleanup
      done
    done
  done
done
