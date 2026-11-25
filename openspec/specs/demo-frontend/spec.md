# demo-frontend Specification

## Purpose

A local web frontend for demonstrations, as a real browser client of the API. A person signs in with Keycloak, uploads a PDF, reads its status and searches (tech design 0065, ADR 0013).

## Requirements

### Requirement: The site has one page for each feature

The site SHALL be static, and SHALL have the pages `/` (sign-in and sign-out, with the tenant and the `sub` of the token), `/upload`, `/documents` and `/search`. It SHALL load no script from another origin. It SHALL send no telemetry and no `traceparent`.

#### Scenario: The site builds

- **WHEN** CI runs the check and the build of the site
- **THEN** both pass, and the output has the four pages

### Requirement: The site signs in with the code flow and PKCE

The site SHALL sign in through the realm `strata` of the local Keycloak, as the public client `strata-web`, with the authorization code flow and PKCE `S256`. It SHALL keep the token in memory and in `sessionStorage`, and SHALL renew it before it ends. It SHALL send the access token only in the `Authorization` header of its calls to the API, and SHALL send no cookie to the API.

#### Scenario: TC-65-06 The realm file

- **GIVEN** `dev/keycloak/strata-realm.json`
- **WHEN** the test reads it
- **THEN** the client `strata-web` is public, has only the standard flow, requires PKCE `S256`, and has only redirect URIs and web origins on `http://localhost:4321`
- **AND** each user has a fixed `id` and a `tenant` attribute
- **AND** the access token lifetime is not more than 300 seconds

### Requirement: The site shows the answers of the API as they are

The site SHALL call the API at `http://127.0.0.1:8080`. For an error, it SHALL show the status code and the error body of the API. It SHALL not retry a call, except a status read after its `Retry-After`.

#### Scenario: An error of the API

- **GIVEN** a signed-in person on `/search`
- **WHEN** the API answers a search with a status code of an error
- **THEN** the page shows that status code and the error body

### Requirement: The site remembers its uploads for each person

The page `/upload` SHALL do the start, the PUT to the signed URL with `Content-Type: application/pdf`, and the finish of an upload. It SHALL also start a new revision of a document that the site knows. The site SHALL keep each `(document_id, revision)` that it uploaded in `localStorage`, under one key for each issuer, tenant and `sub`. The page `/documents` SHALL read their status with the batch status, and SHALL read a status that is not final again after its `Retry-After`.

#### Scenario: Two people in one browser

- **GIVEN** `alice@acme` uploaded a document in a browser, and signed out
- **WHEN** `alice@globex` signs in in the same browser and opens `/documents`
- **THEN** the page does not list the document of `alice@acme`

### Requirement: The browser flow works end to end

A browser test SHALL drive the site through the sign-in, an upload, the status and a search, for two tenants.

#### Scenario: TC-65-07 From sign-in to search in the browser

- **PENDING** It runs locally with Playwright. CI cannot start Keycloak yet (#141).
- **GIVEN** `make dev` and `make web`, run locally
- **WHEN** `alice@acme` signs in, uploads `two-pages.pdf`, waits on `/documents` until its status is final, and searches for a word of it
- **THEN** the status is `ready`, and the search has a hit of the document
- **WHEN** `alice@globex` signs in in a new browser context and searches for the same word
- **THEN** the search has no hit

### Requirement: The search page shows each hit with its snippet

The page `/search` SHALL show one row for each hit of the response, in the order of the response, with the location and the snippet of the hit. It SHALL put the text of each segment in the page as text, never as HTML. It SHALL show a segment with `match` in bold.

#### Scenario: A hit with a matched word

- **GIVEN** a signed-in person on `/search`, and a document of the tenant with the word `bravo` on one page
- **WHEN** the person searches for `bravo`
- **THEN** the page shows one row for that page
- **AND** the row shows the words around `bravo`, with `bravo` in bold

#### Scenario: A snippet that holds markup

- **GIVEN** a hit whose segments hold the text `if x < y && a > b`
- **WHEN** the page shows the hit
- **THEN** the page shows the characters of the text, and it adds no element for them
