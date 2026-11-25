#!/usr/bin/env bash
# make web: the frontend for demonstrations locally (tech design 0065, spec dev-services).
# It needs the processes of make dev. It starts Keycloak (the compose profile frontend), registers
# its issuer and binds the tenants acme and globex to it, applies the CORS rule of
# dev/garage-cors.xml to the bucket, and runs the Astro dev server on http://localhost:4321.
# A second run skips each row that exists. Ctrl-C stops the dev server. Keycloak runs on.
set -euo pipefail
cd "$(dirname "$0")/.."

site=http://localhost:4321
api=http://127.0.0.1:8080
kc=http://localhost:8180/realms/strata

# Any HTTP answer means that the API listens. The site needs its origin in STRATA_CORS_ORIGINS.
curl -s -o /dev/null "$api/v1/search" || {
  echo "error: the API does not answer on $api. Run make dev first." >&2
  exit 1
}
curl -s -D - -o /dev/null -X OPTIONS -H "Origin: $site" -H 'Access-Control-Request-Method: QUERY' \
  "$api/v1/search" | grep -qi "^access-control-allow-origin: $site" || {
  echo "error: the API does not allow the origin $site. Run make dev again (it sets STRATA_CORS_ORIGINS)." >&2
  exit 1
}

# podman-compose stays attached on a first start without -d.
${COMPOSE:-docker compose} --profile frontend up -d keycloak
for _ in $(seq 120); do
  curl -sf -o /dev/null "$kc/.well-known/openid-configuration" && break
  sleep 1
done
curl -sf -o /dev/null "$kc/.well-known/openid-configuration" || {
  echo "error: Keycloak does not answer on localhost:8180" >&2
  exit 1
}

# The tenants acme and globex come from make dev.
admin() { STRATA_DATABASE_URL=postgres://strata_operator:strata_operator@localhost:5432/strata target/debug/strata admin "$@"; }
issuer=(--jwks-url "$kc/protocol/openid-connect/certs" --audience strata --algorithm RS256
  --tenant-claim tenant)
admin issuer register --issuer "$kc" "${issuer[@]}" 2>/dev/null || admin issuer set "$kc" "${issuer[@]}"
for t in acme globex; do
  admin binding remove --issuer "$kc" --tenant-value "$t" 2>/dev/null || true
  admin binding add --issuer "$kc" --tenant-value "$t" --tenant "$t"
done

# The development key of compose.yaml.
curl -sf -o /dev/null --aws-sigv4 aws:amz:garage:s3 \
  --user GK000000000000000000000000:0000000000000000000000000000000000000000000000000000000000000000 \
  -X PUT -H 'Content-Type: application/xml' --data-binary @dev/garage-cors.xml \
  'http://localhost:3900/strata?cors'

cd web
# npm writes node_modules/.package-lock.json at each install.
[ node_modules/.package-lock.json -nt package-lock.json ] || npm ci --no-fund
echo "ok: the site on $site. Sign in as alice@acme, bob@acme or alice@globex (dev/README.md)."
ASTRO_TELEMETRY_DISABLED=1 exec npx astro dev
