# Spike #100: sqlx against tokio-postgres

Throwaway. This branch is the record of the spike. The write-up is in issue #100.

```sh
export BENCH_PASSWORD=$(openssl rand -hex 16)   # never committed
printf '"bench_app" "%s"\n' "$BENCH_PASSWORD" > userlist.txt   # git-ignored
podman-compose -p strata-bench up -d
podman exec strata-bench_postgres_1 psql -q -U bench_owner -d bench -c "alter role bench_app password '$BENCH_PASSWORD'"
podman exec -i strata-bench_postgres_1 psql -q -U bench_owner -d bench < seed.sql
curl -XPOST localhost:58474/proxies -d '{"name":"pg","listen":"0.0.0.0:5433","upstream":"postgres:5432"}'
curl -XPOST localhost:58474/proxies/pg/toxics -d '{"name":"rtt","type":"latency","stream":"downstream","attributes":{"latency":1}}'
cargo build --release
./run.sh 5 10 direct pgbouncer latency1ms > results.jsonl
python3 summary.py results.jsonl > summary.md
podman-compose -p strata-bench down -v
```

- `results.jsonl` has one line for each run of each cell. `summary.md` has the medians, the spread and the 20 % rule.
- Libraries:
  - `sqlx` uses the pool of sqlx.
  - `tp` uses tokio-postgres with deadpool, one statement after the other.
  - `tp-pipe` is the same, but it sends the four statements of the tenant transaction together.
- The runs use 64 client tasks on 16 connections, 2 s of warm-up and 10 s of measurement. The order of the libraries changes on each run.
- Round trips: `BENCH_POOL=1 BENCH_TASKS=1` against the 1 ms path gives about 1.1 ms for each round trip.
