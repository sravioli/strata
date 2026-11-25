---
status: "accepted"
date: 2026-11-25
decision-makers: sravioli
tech-design: docs/tech-design/0065-demo-frontend.md
---

# The API serves browser clients through exact CORS origins, and a local frontend uses Keycloak

## Context and Problem Statement

A person can use the API of Strata only with `curl`, `hurl` or a tool like it. A frontend for demonstrations must sign in, upload, show the status and search, as a real browser client. Which identity provider issues its tokens, and how does a browser call the API and the bucket across origins? The issue is [#65](https://github.com/sravioli/strata/issues/65).

## Considered Options

* Keycloak with the code flow and PKCE in the browser, and real CORS on the API and the bucket
* Tokens from `strata-dev` for the browser
* A backend for the frontend that holds the token in a cookie
* A proxy in the dev server, with the same origin for the site, the API and the bucket

## Decision Outcome

Chosen option: "Keycloak with the code flow and PKCE in the browser, and real CORS on the API and the bucket". It tests the real case of a browser client. That is a sign-in, the key of a real provider, the claims of a mapper and calls across origins.

The decision has these parts:

* The site is static Astro in `web/`, local only, with one page for each feature of the API.
* Keycloak 26.6 runs in `start-dev` with no volume, in the compose profile `frontend`, and imports its realm from a file. A user attribute `tenant` gives a string claim `tenant`, and a mapper adds `strata` to `aud`. The rules of tokens of ADR 0008 (identity providers) do not change.
* The API gets the setting `STRATA_CORS_ORIGINS`, a list of exact origins. With no value, the API has no CORS layer. A bad origin stops the start. The layer sends no `Access-Control-Allow-Credentials`.
* The Garage bucket gets a CORS rule for the PUT from the site.
* The site polls the status with `Retry-After`, and remembers its uploads in `localStorage` until a list route exists ([#140](https://github.com/sravioli/strata/issues/140)).
* `strata-dev` stays for `make hurl` and the tests.

### Consequences

* Good, because the API now has a defined and tested way to serve browser clients, off by default.
* Good, because the tests and CI do not need Keycloak.
* Bad, because `make web` starts up to 1.5 GiB more of containers.
* Bad, because the browser test runs only locally until Keycloak runs in CI ([#141](https://github.com/sravioli/strata/issues/141)).
* Bad, because a hosted demonstration needs its own design ([#142](https://github.com/sravioli/strata/issues/142)).
