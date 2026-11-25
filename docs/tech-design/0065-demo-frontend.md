- Issue: [#65](https://github.com/sravioli/strata/issues/65)
- Status: accepted
- Start date: 2026-11-25
- Decided by: sravioli, 2026-11-25

# Frontend for demonstrations: Astro, Keycloak and CORS, local

## Problem description

Today, a person can use the API of Strata only with `curl`, `hurl` or a tool like it. To upload a document, they must get a token from `strata-dev`, call three routes and do a PUT to a signed URL. This design adds a small web frontend. With it, a person signs in, uploads a PDF, sees its status and searches. It runs locally, next to `make dev`. It is a real browser client: it gets its token from a real OIDC provider, and it calls the API across origins, with CORS.

Goals:

- A static site in `web/`, built with Astro. It has one page for each feature of the API of today: sign-in, upload, status and search.
- A local Keycloak issues the tokens. Its realm has two tenants, so the site can show that one tenant does not see the documents of a different tenant.
- The API and the Garage bucket accept the calls of the site across origins, through an exact list of origins.
- One command, `make web`, starts it.

Non-goals:

- A hosted demonstration, with a deployed Strata and a production identity provider ([#142](https://github.com/sravioli/strata/issues/142)).
- Features that the API does not have: download ([#118](https://github.com/sravioli/strata/issues/118)), removal ([#69](https://github.com/sravioli/strata/issues/69)), version restore ([#101](https://github.com/sravioli/strata/issues/101)) and tenant administration ([#66](https://github.com/sravioli/strata/issues/66)). The site gets a page for each when the API gets the feature.
- Status push to the browser ([#55](https://github.com/sravioli/strata/issues/55)). The site polls.
- A change to the rules of tokens of tech design 0021.

## Background and references

- Strata issues no tokens. It verifies the tokens of trusted issuers (tech design 0021, ADR 0008 (identity providers)). A token must have `iss`, `exp`, `iat` and `sub`. Its `aud` must hold the audience of its issuer, and its lifetime must not be more than `max_lifetime_s`. Its tenant claim must be a string of 1 to 256 bytes. The API gets the key from the JWKS URL of the issuer. It keeps only keys with the `kty` of the algorithm of the issuer and with `use` `sig` or no `use` (`select_keys` in `crates/strata/src/auth/keys.rs`).
- `scripts/dev.sh` registers the development issuer `strata-dev` and binds the tenants `acme` and `globex` to it. `make hurl` and the L1 and L2 tests use its tokens.
- The routes of today (tech designs 0006, 0047 and 0049): `POST /v1/documents`, `POST /v1/documents/{document_id}/revisions`, `POST .../revisions/{revision}/finish`, `GET .../revisions/{revision}`, and the method `QUERY` on `/v1/revisions` (batch status) and on `/v1/search`. A status that is not final has a `Retry-After` header. The API has no route that lists the documents of a tenant.
- A start of an upload gives a signed PUT URL of Garage (`http://localhost:3900` locally). The URL signs only the host. `dev/strata.hurl` sends the PUT with `Content-Type: application/pdf`.
- CORS: a browser sends a preflight `OPTIONS` before some requests across origins. It sends one for a method other than `GET`, `HEAD` or `POST`, and for a header that is not on the safelist. So `QUERY`, `Authorization` and `Content-Type: application/pdf` each cause a preflight. JS in the browser can read a response header only if `Access-Control-Expose-Headers` names it, apart from a short safelist that does not include `Retry-After`.
- Garage accepts a CORS rule for a bucket through the S3 call `PutBucketCors`.
- OAuth 2.0 for browser apps: RFC 9700, and the IETF draft "OAuth 2.0 for Browser-Based Applications". A public client uses the authorization code flow with PKCE.
- Keycloak 26.6 ([containers guide](https://www.keycloak.org/server/containers)). `start-dev` runs it in development mode. `--import-realm` reads the realm files of `/opt/keycloak/data/import` at start, and skips a realm that exists ([import and export](https://www.keycloak.org/server/importExport)). Since Keycloak 24, the heap is 70% of the memory limit of the container. Keycloak puts the hostname of the request in `iss`. The setting `KC_HOSTNAME` replaces it with a fixed value.
- Astro builds static HTML pages. Astro bundles each `<script>` of a page as a module for the browser. `oidc-client-ts` is a library for the OIDC code flow with PKCE in the browser.

## Requirements

The owner decided these points in #65 on 2026-11-25:

1. **Scope.** The site covers each feature that the API has now: upload, status and search, for two tenants. More pages come when the API gets more features.
2. **Local only** for now.
3. **A real issuer.** The site gets its tokens from a real OIDC provider, not from `strata-dev`, so it tests the real case. The provider is Keycloak.
4. **Astro**, with static output.
5. **Polling** for the status, with `Retry-After`.
6. **The tenant in the token** comes from a user attribute `tenant`, which a mapper puts in the access token as a string claim `tenant`. A second mapper adds `strata` to `aud`. The rules of the API do not change.
7. **The code flow with PKCE in the browser**, as a public client, with `oidc-client-ts`. The site has no server of its own.
8. **Real CORS** on the API and on the Garage bucket. No proxy that hides the origins.
9. **`web/`** in the repo, with its npm packages pinned in `web/package-lock.json`, and a target `make web`. `make dev` does not change.
10. **`strata-dev` stays** for `make hurl` and the tests. Only the site uses Keycloak.
11. **CI** runs `astro check` and `astro build`. The browser flow is a test case with a pending marker.
12. **The CORS setting** of the API is `STRATA_CORS_ORIGINS`, a list of exact origins, empty by default.
13. **Garage** gets its CORS rule from a file and one `curl --aws-sigv4` call.
14. **Fixed ports** and the hostname `localhost` (see "Proposed design").
15. **Keycloak** runs in `start-dev` with no volume, and imports its realm at each start of a new container.
16. **The site remembers its uploads** in `localStorage`, because the API has no list route.
17. **Plain pages**: one Astro page for each feature, plain CSS, TypeScript in `<script>`, no UI framework.
18. **No telemetry** from the site. The API shows each error as it is.

Further constraints:

- **No change in production by default.** With `STRATA_CORS_ORIGINS` unset or empty, the API adds no CORS layer, and each response is as today.
- **The volume is small.** One person clicks, locally. Each page polls one status at a time, at the interval of `Retry-After`.
- **Development secrets only.** The passwords of the seed users and the Garage key are development values in the repo, like the values of `compose.yaml` today.

## Proposed design

```text
browser http://localhost:4321 (Astro dev server, make web)
  │ 1. code flow with PKCE           ┌──────────────────────────────────┐
  ├─────────────────────────────────►│ Keycloak :8180, realm strata     │
  │    access token (RS256)          └──────────────▲───────────────────┘
  │ 2. Bearer token, CORS                           │ JWKS
  ├─────────────────────────────────► strata api :8080 ─┘
  │ 3. PUT to the signed URL, CORS
  └─────────────────────────────────► Garage :3900, bucket strata
```

**The site.** `web/` is an Astro project with static output. Its pages:

- `/`: sign-in and sign-out. It shows the tenant and the `sub` of the token.
- `/upload`: a file input for one PDF. The page calls `POST /v1/documents`, does the PUT to the signed URL with `Content-Type: application/pdf`, and calls `finish`. A second form uploads a new revision of a document that the page knows.
- `/documents`: the uploads that the page remembers, with their status from `QUERY /v1/revisions`. The page reads a status that is not final again after its `Retry-After`.
- `/search`: a text field and a limit. It shows each hit of `QUERY /v1/search` with its text.

The site calls the API at `http://127.0.0.1:8080`, the address of `make dev`. The API listens only on IPv4, and a browser can try `::1` first for `localhost`. Each page is one `.astro` file with plain CSS and a `<script>` in TypeScript. One small module, `web/src/api.ts`, sends each call with the token and gives the status code and the error body of the API as they are. One module, `web/src/auth.ts`, holds the `oidc-client-ts` client. The token is in memory and in `sessionStorage`, so it ends with the tab. `oidc-client-ts` renews it with the refresh token before it ends. The site remembers each `(document_id, revision)` that it uploaded in `localStorage`, under one key for each issuer, tenant and `sub`. So a person of `globex` does not see the list of a person of `acme` in the same browser. The site sends no telemetry and no `traceparent`.

**Keycloak.** A new service `keycloak` in `compose.yaml`, in a new compose profile `frontend`. So `make l1`, `make dev` and CI do not start it.

- The image is `quay.io/keycloak/keycloak` 26.6.x, pinned by tag and digest. The command is `start-dev --import-realm`.
- It listens on `127.0.0.1:8180`. `KC_HOSTNAME` is `http://localhost:8180`, so `iss` is always `http://localhost:8180/realms/strata`.
- It has no volume. A new container imports `dev/keycloak/strata-realm.json`. A change to that file takes effect after `podman compose up -d --force-recreate keycloak`. `dev/README.md` gives this command.
- Its memory limit is 1.5 GiB.

The realm `strata` has:

- One public client `strata-web`: the standard flow only, PKCE with `S256` required, the redirect URI `http://localhost:4321/*`, and the web origin `http://localhost:4321`.
- A user attribute `tenant`, and a mapper that puts it in the access token as the string claim `tenant`.
- An audience mapper that adds `strata` to `aud` of the access token.
- Three users: `alice@acme` and `bob@acme` with `tenant` `acme`, and `alice@globex` with `tenant` `globex`. Each has a fixed development password and a fixed `id`. Keycloak uses the `id` as `sub`. So a new container gives the same `sub`, the API finds the same account, and the list of uploads of the site stays.
- An access token lifetime of 5 minutes.

**The issuer row.** `make web` registers Keycloak, as `scripts/dev.sh` does for `strata-dev`: the issuer `http://localhost:8180/realms/strata`, the JWKS URL `http://localhost:8180/realms/strata/protocol/openid-connect/certs`, the audience `strata`, the algorithm `RS256`, and the tenant claim `tenant`. It binds the tenant values `acme` and `globex` to the tenants `acme` and `globex`. A second run skips each row that exists. The JWKS of Keycloak also has a key for encryption, with `use` `enc`. `select_keys` keeps only the signing key. A new container of Keycloak makes a new signing key with a new `kid`. The `KeyCache` of the API gets the JWKS again for a `kid` that it does not have, at most once in 60 s (`UNKNOWN_KID_RETRY`). So a sign-in after a new container works, at the latest after 60 s.

**CORS on the API.** A new setting `STRATA_CORS_ORIGINS`: a list of origins, separated by commas. Each item must be an exact origin: `http` or `https`, a host, an optional port, and no path, query, fragment, user information, trailing slash or wildcard. A bad item stops `strata api` at start with a message that names the setting, like the other settings. When the list is empty or unset, the router has no CORS layer. When it has items, the router gets the `CorsLayer` of `tower-http` as its outermost layer. So a preflight gets its answer before authentication, the bound of requests and the rate limits. The layer:

- It allows the origins of the list, and no other origin.
- It allows the methods `GET`, `POST` and `QUERY`.
- It allows the request headers `authorization` and `content-type`.
- It exposes the response header `retry-after`.
- It sends `Access-Control-Max-Age: 600`.
- It sends no `Access-Control-Allow-Credentials`, because the token is in a header and not in a cookie.
- It sends `Vary: Origin`.

Locally, `strata api` runs with `STRATA_CORS_ORIGINS=http://localhost:4321` (see "`make web`").

**CORS on Garage.** `dev/garage-cors.xml` holds one CORS rule: the origin `http://localhost:4321`, the method `PUT`, the header `content-type`, and a max age of 600 s. `make web` applies it with one `curl --aws-sigv4` call to `PutBucketCors` on the bucket `strata`, with the development key of `compose.yaml`. A bucket of a deployed system needs the same rule with its own origins. The design of the hosted demonstration decides it.

**`make web`.** It starts the profile `frontend` and waits until Keycloak answers. Then it registers the issuer and the bindings, applies the CORS rule of Garage, and runs the Astro dev server on `localhost:4321`. It needs a `strata api` that runs with `STRATA_CORS_ORIGINS=http://localhost:4321`. How that `strata api` starts is an open point of the implementation (see "Unresolved questions"). The docs, the realm and the scripts always use `localhost`, not `127.0.0.1`, because the origin and the issuer are exact matches.

**CI.** A new job `web` runs `npm ci`, `astro check` and `astro build` in `web/`, when a file in `web/` changes. The job `lint` also reads the YAML and Markdown files of `web/`.

## Alternatives considered

**Tokens from `strata-dev`.** A small endpoint of `strata-dev` could give a token for any tenant and `sub`. It needs no Keycloak and no new container. But then the site would not test the real case. The real case is a sign-in, an RS256 key of a real provider, the claims of a mapper and a renewal. The owner chose a real issuer (decision 3). `strata-dev` stays for the tests.

**Other OIDC providers.** Zitadel puts the ID of the organization in the token as a string, which fits the tenant claim with no mapper. But its setup is harder to script and fewer people know it. Authentik needs Python property mappings for a custom claim. Dex cannot add a custom tenant claim. Keycloak is the most common provider, and a realm file makes it the same at each start. Keycloak Organizations puts the organization in the token as an object or a list, not a string. That needs a change to the claim rules of the API, so the design uses a user attribute.

**A backend for the frontend.** An Astro server endpoint could hold the token in an `HttpOnly` cookie and send each call to the API. That protects the token better from injected scripts. But the site would then be a server, and the browser would not call the API, so CORS would not be tested. The owner chose the code flow in the browser (decision 7).

**A proxy in the dev server instead of CORS.** The Astro dev server could send `/v1` and the PUT to Garage from its own origin. That needs no change to the API. But the site must then change the host of the signed URL, and the proxy must send the `Host` header that the signature covers. A deployed browser client needs CORS anyway. The owner chose real CORS (decision 8).

**Keycloak data in our PostgreSQL, or Keycloak in production mode.** With persisted data, `--import-realm` skips the realm, so a change to the realm file has no effect until a person deletes the realm by hand. It also adds a database and a role for Keycloak to `dev/roles.sql`, which `make sql` checks. Production mode (`start`) also needs a build step, a strict hostname and http enabled on purpose. None of these changes what Strata sees: the token and the JWKS. The owner chose `start-dev` with no volume (decision 15).

**A list route in the API now.** The page `/documents` could read the documents of the tenant from the API. But the route needs its own design: paging, and which documents a caller can list. It is a separate issue. The site uses `localStorage` until then (decision 16).

**A plain HTML file instead of Astro.** One HTML file with inline JS needs no Node build. But it becomes hard to read after some pages, and the site will grow. The owner chose Astro (decision 4).

## Proof obligations

N/A. This design adds no function to `crates/core` and no domain rule. L0 tests cover the parse of `STRATA_CORS_ORIGINS` and the checks of the realm file. L1 tests cover the CORS answers of the API. The browser flow is a test case with a pending marker (see Test cases).

## Impacts

### Infrastructure

- New local service only: `keycloak`, in the compose profile `frontend`, on `127.0.0.1:8180`, with a memory limit of 1.5 GiB and no data on disk.
- The Astro dev server listens on `localhost:4321`.
- A new CI job `web`. No deployed infrastructure.
- To roll back: delete `web/`, the profile and the target. With `STRATA_CORS_ORIGINS` unset, the API is as before.

### Security and tenant isolation

- **CORS does not authorize.** CORS only lets JS of an allowed origin read a response. Each request still needs a valid token, and the tenant still comes only from the token. A request from an origin that is not on the list gets the same answer as today, with no CORS headers. So the browser does not show the answer to the script.
- **Exact origins.** No wildcard, no pattern and no `null` origin. The API does not echo an origin that is not on the list. A bad item stops the start, so a typing error cannot open the API to more origins.
- **No credentials mode.** The API sends no `Access-Control-Allow-Credentials`, and the site sends no cookies to the API. The token goes only in `Authorization`.
- **Preflight before authentication.** The CORS layer answers a preflight before the bound of requests and the rate limits. A preflight has no body and reads no database. Each request after the preflight passes the same layers as today.
- **Default off.** A deployed API with no `STRATA_CORS_ORIGINS` has no CORS layer. A deployed site needs its own design, with its own origins.
- **The token in the browser.** A public client has no secret. PKCE `S256` protects the code. The token is in `sessionStorage`, so an injected script in the site can read it. The site loads no script from a different origin, and Astro builds all scripts into the site. The access token ends after 5 minutes.
- **Keycloak** is a development service. Its admin console uses a development password, and its port binds to `127.0.0.1`. The passwords of the seed users are in the realm file, and `.betterleaksignore` lists them like the other development values.
- **The Garage rule** lets JS of `http://localhost:4321` do a PUT. The PUT still needs a valid signed URL, which only the API gives, for one key and for 15 minutes.
- **New dependencies:** `tower-http` with the feature `cors` in the API, and the npm packages of `web/` (Astro, `oidc-client-ts` and their dependencies). `cargo deny` checks the crate. `web/package-lock.json` pins the npm packages. The implementation also runs `npm audit` on them once.

### Data and migration

- No schema change. `make web` adds one issuer row and two binding rows to the local database, like `scripts/dev.sh` does today.
- The site keeps data only in the browser: the token in `sessionStorage`, and the IDs of its uploads in `localStorage`.

### Developer

- `make web` needs Node and npm, like `make tools` today, and the processes of `make dev`.
- Keycloak takes some seconds to start, and up to 1.5 GiB of memory.
- To sign in: `alice@acme`, `bob@acme` or `alice@globex`, with the passwords in `dev/README.md`.
- `make hurl` and the tests do not change.

## Test cases

L0 tests cover the parse of `STRATA_CORS_ORIGINS`: each kind of bad item, and a list with spaces around the commas. A test in `scripts/tests` reads the realm file. L1 tests send requests to the router with and without the setting.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-65-01 | L1 | Given `STRATA_CORS_ORIGINS` is not set. When a client sends a preflight `OPTIONS /v1/search` with an `Origin`, and a `GET` of a status with an `Origin`. Then no response has an `Access-Control-` header, and the `GET` gets the same answer as with no `Origin`. | sravioli, 2026-11-25 | pending |
| TC-65-02 | L1 | Given `STRATA_CORS_ORIGINS=http://localhost:4321`. When a client sends a preflight for `QUERY /v1/search` from that origin, with the headers `authorization` and `content-type`. Then the answer has `Access-Control-Allow-Origin: http://localhost:4321`, allows `GET`, `POST` and `QUERY` and the two headers, has `Access-Control-Max-Age: 600` and has no `Access-Control-Allow-Credentials`. | sravioli, 2026-11-25 | pending |
| TC-65-03 | L1 | Given the same setting. When a client sends a preflight and a `GET` of a status from `http://localhost:4322`, and from the origin `null`. Then no answer has `Access-Control-Allow-Origin`. | sravioli, 2026-11-25 | pending |
| TC-65-04 | L1 | Given the same setting. When an account of the allowed origin reads the status of a revision that is not final. Then the answer has `Retry-After` and `Access-Control-Expose-Headers` that names `retry-after`. | sravioli, 2026-11-25 | pending |
| TC-65-05 | L1 | Given `STRATA_CORS_ORIGINS` holds `*`, `http://localhost:4321/`, `http://localhost:4321/app`, `localhost:4321` or `ftp://localhost`. When `strata api` starts. Then it stops with a message that names `STRATA_CORS_ORIGINS`. | sravioli, 2026-11-25 | pending |
| TC-65-06 | L0 | Given `dev/keycloak/strata-realm.json`. When the test reads it. Then the client `strata-web` is public, has only the standard flow, requires PKCE `S256` and has only redirect URIs and web origins on `http://localhost:4321`. Each user has a fixed `id` and a `tenant` attribute, and the access token lifetime is not more than 300 s. | sravioli, 2026-11-25 | pending |
| TC-65-07 | L2 | Given `make dev` and `make web`, run locally. When `alice@acme` signs in, uploads `two-pages.pdf`, waits on `/documents` until its status is final, and searches for a word of it. Then the status is `Done` and the search has a hit of the document. When `alice@globex` signs in in a new browser context and searches for the same word. Then the search has no hit. | sravioli, 2026-11-25 | pending |

TC-65-07 runs locally with Playwright. It has a pending marker in CI until CI can start Keycloak (see "Unresolved questions").

## Unresolved questions

The owner answered the questions of #65 on 2026-11-25 (see "Requirements").

New issues, linked from #65:

- [#140](https://github.com/sravioli/strata/issues/140): a route that lists the documents of the caller, with paging. Then `/documents` reads it, not `localStorage`.
- [#141](https://github.com/sravioli/strata/issues/141): Keycloak in CI, so TC-65-07 runs there.
- [#142](https://github.com/sravioli/strata/issues/142): a hosted demonstration, with a deployed Strata, a production identity provider, the origins of the site and the CORS rule of its bucket.

During implementation:

- The versions of Astro, `oidc-client-ts` and `tower-http`, the digest of the Keycloak image, and the Node version of the job `web`.
- Whether Garage v2.5 accepts `PutBucketCors` from `curl --aws-sigv4`, and whether it needs `Content-MD5`.
- How `make web` starts `strata api` with `STRATA_CORS_ORIGINS`: `make web` runs `scripts/dev.sh` with the variable set, or `scripts/dev.sh` always sets it when the variable `CI` is not set.
- Whether the realm file must declare the attribute `tenant` in the user profile of the realm. Since Keycloak 24, the user profile is always on, and by default it does not keep an attribute that it does not declare. The implementation checks this on the pinned image.
- Whether `tower-http` accepts `QUERY` as a method of `AllowMethods`. It takes any `http::Method`, and `Method::from_bytes(b"QUERY")` gives one.
