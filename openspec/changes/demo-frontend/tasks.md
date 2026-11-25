# Tasks

## 1. Facts to find first

- [x] 1.1 Find the newest Keycloak 26.6.x image and its digest. Start it in `start-dev` with a realm that has a user with the attribute `tenant` and no user profile entry for it. Find out whether the import keeps the attribute, and whether a mapper puts it in the access token. If it does not, find the smallest user profile entry that keeps the attribute. Write the result in the pull request. Verify: an access token of the test user has the string claim `tenant`.
- [x] 1.2 Send `PutBucketCors` with `curl --aws-sigv4` to the Garage of `compose.yaml` (v2.5), with and without `Content-MD5`. Then send a preflight `OPTIONS` for a PUT from `http://localhost:4321` to a signed URL. Write the result in the pull request. Verify: the preflight gets `Access-Control-Allow-Origin: http://localhost:4321`.

## 2. CORS on the API

- [x] 2.1 Add `tower-http` with only the feature `cors` to `crates/strata`, pinned with `=` like the other dependencies. Verify: `make deny` passes.
- [x] 2.2 Add `crates/strata/src/cors.rs`: the parse of `STRATA_CORS_ORIGINS` into a list of exact origins, with L0 tests. Test each kind of bad item of the spec `cross-origin-access`, an empty value, and spaces around the commas. Verify: the L0 tests pass.
- [x] 2.3 In `api.rs`, read the setting in `state`, so a bad item stops `strata api` before it serves a request. With items, add the `CorsLayer` as the outermost layer of the router. Give it the methods, the headers, the exposed header and the max age of the spec. With no items, add no layer. Verify: the L1 tests TC-65-01 to TC-65-05 in `crates/strata-tests/tests/cors.rs` pass. Remove their pending markers.
- [x] 2.4 In `scripts/dev.sh`, in the branch where `CI` is not set, set `STRATA_CORS_ORIGINS=http://localhost:4321` for `strata api` when the variable is not in the environment. In the branch where `CI` is in the environment, set no value. Verify: the scenarios "A local run allows the site" and "A run in CI has no CORS" of `dev-services` pass, the second one as `CI=1 make l2` locally.

## 3. Keycloak, the Garage rule and make web

- [ ] 3.1 Add the service `keycloak` to `compose.yaml` in the profile `frontend`, with the image of task 1.1 pinned by tag and digest. Give it `start-dev --import-realm`, `127.0.0.1:8180`, `KC_HOSTNAME=http://localhost:8180`, a development admin password, no volume and a memory limit of 1.5 GiB. Verify: the scenario "The default start leaves Keycloak out" passes with Podman, and with Docker where the machine has it.
- [ ] 3.2 Add `dev/keycloak/strata-realm.json` with the realm `strata`, the client `strata-web`, the mappers of `tenant` and of the audience `strata`, and the three seed users with fixed IDs and development passwords. Use the result of task 1.1 for the user profile. Set the access token lifetime to 300 seconds. Add `scripts/tests/test_keycloak_realm.py`. Add the passwords to `.betterleaksignore`. Verify: the test TC-65-06 passes, `make secrets` passes, and the scenario "Keycloak issues tokens with the claims of Strata" passes locally, also after `--force-recreate`.
- [ ] 3.3 Add `dev/garage-cors.xml` with the rule of the spec. Verify: with the result of task 1.2, the rule applies with one `curl --aws-sigv4` call.
- [ ] 3.4 Add `scripts/web.sh` and the target `make web`. It reads an answer of the API first, and stops with a message that names `make dev` when it does not. It starts the profile `frontend` and waits until Keycloak answers. It registers the issuer and binds `acme` and `globex` like `scripts/dev.sh`, and applies the Garage rule. Then it runs the Astro dev server on `localhost:4321`. Verify: the scenarios "The first run of make web" and "make web with no API" pass locally. A second run fails on no step.

## 4. The Astro site

- [ ] 4.1 Add `web/`: an Astro project with static output, the newest Astro and `oidc-client-ts`, and `web/package-lock.json`. Add the pages `/`, `/upload`, `/documents` and `/search` with plain CSS and a `<script>` in TypeScript. Add `web/src/auth.ts`: the client `strata-web`, PKCE, the token in memory and in `sessionStorage`, and the renewal. Add `web/src/api.ts`: the calls to `http://127.0.0.1:8080`, with the status code and the error body as they are. Run `npm audit` once and write the result in the pull request. Verify: `astro check` and `astro build` pass, and the output has the four pages.
- [ ] 4.2 In `/upload`, do the start, the PUT with `Content-Type: application/pdf` and the finish, and a new revision of a known document. In `/documents`, keep the uploads in `localStorage` under one key for each issuer, tenant and `sub`. Read them with the batch status. Read a status that is not final again after its `Retry-After`. In `/search`, show each hit with its text, and show an error as the status code and the body. Verify: by hand after `make dev` and `make web`, the scenarios "An error of the API" and "Two people in one browser" pass.
- [ ] 4.3 Add the Playwright test of TC-65-07 in `web/tests/`, with a script `npm run test:e2e`. Keep its pending marker, because CI cannot start Keycloak (#141). Verify: the test passes locally after `make dev` and `make web`.
- [ ] 4.4 Add the CI job `web`: `npm ci`, `astro check` and `astro build` in `web/`, when a file in `web/` changes (a new filter in the job `changes`). The job that gates the pull request needs it. Let the job `lint` read the YAML and Markdown files of `web/`. Verify: the job `web` passes on the pull request.

## 5. Docs and gates

- [ ] 5.1 Document `make web`, the seed users and their passwords, the ports, and the command that applies a change to the realm file (`podman compose up -d --force-recreate keycloak`) in `dev/README.md`. Verify: `make lint` passes.
- [ ] 5.2 Run `make`, `make l1`, `CI=1 make l2`, `make l2` and `make deny`. Verify: each target passes, the coverage of `CONSTRAINTS.md` is at the floor or above, and `python3 scripts/check-test-cases.py --report` shows TC-65-01 to TC-65-06 as `implemented` and TC-65-07 as `planned`. If the coverage is below the floor, add tests. Do not lower the number.
- [ ] 5.3 Comment on #65 with the results of tasks 1.1, 1.2 and the `npm audit` of task 4.1. Verify: the comment exists.

## Workflow follow-up

- After the owner merges the pull request, archive the change with `openspec-archive-change`, and close #65.
