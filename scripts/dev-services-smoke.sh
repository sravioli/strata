#!/usr/bin/env bash
# Tests the services of compose.yaml (spec dev-services). Run after: $COMPOSE up --wait
# Needs psql, the aws CLI, python3, and docker compose or podman compose. COMPOSE names the
# command (make l1 sets it). The default is docker compose.
set -euo pipefail

# The development values of compose.yaml.
export PGPASSWORD=strata AWS_ACCESS_KEY_ID=GK000000000000000000000000 \
  AWS_SECRET_ACCESS_KEY=0000000000000000000000000000000000000000000000000000000000000000 \
  AWS_DEFAULT_REGION=garage
s3=(aws --endpoint-url http://localhost:3900 s3)

psql -h localhost -p 5432 -U strata -d strata -tAc 'select 1' | grep -qx 1
echo "ok: PostgreSQL accepts a connection on localhost:5432"

psql -h localhost -p 5432 -U strata -d strata -tAc 'show server_version_num' | grep -q '^19'
echo "ok: the server is PostgreSQL 19"

# Each role of dev/roles.sql logs in with its development password, which is its name.
for role in strata_owner strata_api strata_worker strata_operator; do
  PGPASSWORD=$role psql -h localhost -p 5432 -U "$role" -d strata -tAc 'select current_user' | grep -qx "$role"
done
echo "ok: the four roles log in"

# The runtime roles connect through PgBouncer and run a prepared statement in a transaction.
# A SQL PREPARE stays on the server connection that PgBouncer gives to the next client, so each
# run uses its own name and removes the statement.
p=smoke_$$
for role in strata_api strata_worker; do
  PGPASSWORD=$role psql -h localhost -p 6432 -U "$role" -d strata -v ON_ERROR_STOP=1 -tA \
    -c 'begin' -c "prepare $p(int) as select \$1 + 1" -c "execute $p(1)" -c "deallocate $p" \
    -c 'commit' | grep -x 2 >/dev/null
done
echo "ok: PgBouncer accepts the runtime roles on localhost:6432"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
head -c 4096 /dev/urandom >"$tmp/in"
"${s3[@]}" cp --quiet "$tmp/in" s3://strata/smoke
"${s3[@]}" cp --quiet s3://strata/smoke "$tmp/out"
"${s3[@]}" rm --quiet s3://strata/smoke
cmp "$tmp/in" "$tmp/out"
echo "ok: Garage writes an object and reads back the same bytes"

${COMPOSE:-docker compose} ps --format json | python3 -c '
import json, sys
text = sys.stdin.read().strip()
rows = json.loads(text) if text.startswith("[") else [json.loads(l) for l in text.splitlines()]
# (service, host address) of each published port. Docker Compose gives "Publishers";
# podman compose gives the "Ports" of podman ps.
ports = [(r["Service"], p.get("URL")) for r in rows for p in r.get("Publishers") or []
         if p.get("PublishedPort")]
# In the Docker output "Ports" and "Labels" are text, so only a list is the podman form.
ports += [((r.get("Labels") or {}).get("com.docker.compose.service"), p.get("host_ip"))
          for r in rows if isinstance(r.get("Ports"), list) for p in r["Ports"] if p.get("host_port")]
if not ports:
    sys.exit("fail: no published port in the output of compose ps")
bad = [p for p in ports if p[1] != "127.0.0.1"]
if bad:
    sys.exit(f"fail: ports not bound to 127.0.0.1: {bad}")
'
echo "ok: each published port binds to 127.0.0.1"
