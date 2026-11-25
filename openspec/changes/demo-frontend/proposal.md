Issue: #65

# Proposal

## Why

A person can use the API of Strata only with `curl`, `hurl` or a tool like it. Tech design 0065 (accepted on 2026-11-25, ADR 0013) adds a local frontend for demonstrations, as a real browser client. A person signs in with Keycloak, uploads a PDF, sees its status and searches. The API and the Garage bucket accept its calls across origins through exact CORS origins.

## What Changes

The change has three parts, in this sequence. Each part passes its checks before the next part starts.

1. **CORS on the API.**
   - Add the setting `STRATA_CORS_ORIGINS`: a list of exact origins, separated by commas. A bad item stops `strata api` at start.
   - With no value, the router has no CORS layer, and each response is as today.
   - With a value, the router gets the `CorsLayer` of `tower-http` as its outermost layer. It allows the methods `GET`, `POST` and `QUERY` and the headers `authorization` and `content-type`. It exposes `retry-after`, sends `Access-Control-Max-Age: 600` and sends no `Access-Control-Allow-Credentials`.
2. **Keycloak, the Garage rule and `make web`.**
   - Add the service `keycloak` (Keycloak 26.6, pinned by tag and digest) to `compose.yaml` in a new profile `frontend`. It runs `start-dev --import-realm` with no volume, on `127.0.0.1:8180`, with `KC_HOSTNAME=http://localhost:8180` and a memory limit of 1.5 GiB.
   - Add `dev/keycloak/strata-realm.json`. It has the realm `strata`, the public client `strata-web` with PKCE `S256`, the mappers of `tenant` and of the audience `strata`, and three seed users with fixed IDs. Its access token lifetime is 5 minutes.
   - Add `dev/garage-cors.xml`, a CORS rule for the PUT from `http://localhost:4321`.
   - Add `make web`. It starts the profile, waits for Keycloak, and registers the issuer and the bindings. It applies the Garage rule and runs the Astro dev server on `localhost:4321`.
3. **The Astro site.**
   - Add `web/`: an Astro project with static output and the pages `/`, `/upload`, `/documents` and `/search`. It uses plain CSS and TypeScript in `<script>`, with no UI framework.
   - `web/src/auth.ts` holds the `oidc-client-ts` client: the code flow with PKCE, the token in memory and in `sessionStorage`, and the renewal.
   - `web/src/api.ts` sends each call to `http://127.0.0.1:8080` and gives the status code and the error body as they are.
   - The page `/documents` remembers the uploads in `localStorage`, under one key for each issuer, tenant and `sub`. It polls with `Retry-After`.
   - Add the CI job `web`: `npm ci`, `astro check` and `astro build`, when a file in `web/` changes.
   - Add the Playwright test of TC-65-07. It runs locally, and has a pending marker in CI.

Not in this change:

- A route that lists the documents of the caller ([#140](https://github.com/sravioli/strata/issues/140)).
- Keycloak in CI ([#141](https://github.com/sravioli/strata/issues/141)).
- A hosted demonstration ([#142](https://github.com/sravioli/strata/issues/142)).
- Status push to the browser ([#55](https://github.com/sravioli/strata/issues/55)).
- A change to the rules of tokens of tech design 0021.

## Decisions for this change

The owner decided the points of 0065 on 2026-11-25, with the test cases TC-65-01 to TC-65-07.

The agent decided these points. The owner reviews them in the pull request:

- **Two facts get checked first**, on the pinned images, before the realm file and `make web` exist. First: does Keycloak 26.6 keep the attribute `tenant` with no entry in the user profile of the realm? Second: does Garage v2.5 accept `PutBucketCors` from `curl --aws-sigv4`, and does it need `Content-MD5`? The answers set the content of the realm file and of the call.
- **`STRATA_CORS_ORIGINS` for `make dev`**: `scripts/dev.sh` sets `STRATA_CORS_ORIGINS=http://localhost:4321` for `strata api` when the variable `CI` is not set, in the same branch that sets the telemetry endpoint. A value in the environment wins. So `make web` needs only `make dev`, and CI runs with no CORS layer.
- **The parse of the setting** goes in a new module `crates/strata/src/cors.rs`, next to the settings of `status.rs`, with L0 tests. The L1 tests of TC-65-01 to TC-65-05 go in a new file `crates/strata-tests/tests/cors.rs`.
- **TC-65-07 says `ready`, not `Done`.** `Done` is the state of the job. The status route of the API gives `ready`. The scenario and tech design 0065 (with an amendment note) say `ready`.
- **The test of the realm file** (TC-65-06) is a Python test in `scripts/tests/test_keycloak_realm.py`, so `make test-cases` and CI run it with no Keycloak.

## Test cases

This change carries the test cases of 0065, each as a scenario with its TC ID:

- TC-65-01 to TC-65-05 (L1): the CORS layer of the API.
- TC-65-06 (L0): the realm file.
- TC-65-07 (L2, local with Playwright, pending in CI until #141).

## Capabilities

### New Capabilities

- `cross-origin-access`: the CORS layer of the API: the setting, its parse, the allowed origins, methods and headers, and the default with no layer.
- `demo-frontend`: the site in `web/`: its pages, its sign-in with Keycloak, its calls to the API and to Garage, and what it keeps in the browser. Also its realm file and its browser test.

### Modified Capabilities

- `dev-services`: the profile `frontend` with Keycloak, the Garage CORS rule, `make web`, and the value of `STRATA_CORS_ORIGINS` that `make dev` sets.

## Impact

- `crates/strata/src/`: a new `cors.rs`, and the router in `api.rs`. `Cargo.toml`: `tower-http` with the feature `cors`. `make deny` checks it.
- `crates/strata-tests/tests/cors.rs`: the L1 tests of TC-65-01 to TC-65-05.
- `compose.yaml`, `dev/keycloak/strata-realm.json`, `dev/garage-cors.xml`, `scripts/dev.sh`, a new `scripts/web.sh`, the `Makefile`, `dev/README.md` and `.betterleaksignore`.
- `web/`: the Astro project, `web/package.json` and `web/package-lock.json`, and the Playwright test.
- `.github/workflows/ci.yml`: the job `web`, and its path filter in the job `changes`.
- `scripts/tests/test_keycloak_realm.py`.
- `core` does not change, so the Lean translation and the proofs do not change.
- No schema change.
