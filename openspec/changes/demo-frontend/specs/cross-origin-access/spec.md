# Spec Delta

## Purpose

Lets a browser client of an allowed origin call the API across origins, through an exact list of origins. With an empty list, the API sends no CORS answers (tech design 0065, ADR 0013).

## ADDED Requirements

### Requirement: CORS is off with no origins

`strata api` SHALL read the setting `STRATA_CORS_ORIGINS`. When the setting is unset or empty, the API SHALL send no `Access-Control-` header in any response, and each response SHALL be as with no `Origin` header.

#### Scenario: TC-65-01 No setting, no CORS headers

- **GIVEN** `STRATA_CORS_ORIGINS` is not set
- **WHEN** a client sends a preflight `OPTIONS /v1/search` with an `Origin`, and a `GET` of a status with an `Origin`
- **THEN** no response has an `Access-Control-` header
- **AND** the `GET` gets the same answer as with no `Origin`

### Requirement: The API answers a preflight of an allowed origin

When `STRATA_CORS_ORIGINS` has items, the API SHALL answer a preflight from an origin of the list before authentication and before the limits of requests. The answer SHALL allow that origin, the methods `GET`, `POST` and `QUERY`, and the request headers `authorization` and `content-type`. It SHALL have `Access-Control-Max-Age: 600` and `Vary: Origin`, and SHALL have no `Access-Control-Allow-Credentials`.

#### Scenario: TC-65-02 A preflight of the allowed origin

- **GIVEN** `STRATA_CORS_ORIGINS=http://localhost:4321`
- **WHEN** a client sends a preflight for `QUERY /v1/search` from that origin, with the headers `authorization` and `content-type`
- **THEN** the answer has `Access-Control-Allow-Origin: http://localhost:4321`
- **AND** it allows `GET`, `POST` and `QUERY` and the two headers
- **AND** it has `Access-Control-Max-Age: 600`
- **AND** it has no `Access-Control-Allow-Credentials`

### Requirement: The API allows no other origin

The API SHALL send `Access-Control-Allow-Origin` only with an origin that is equal to an item of the list. It SHALL not send a wildcard, and it SHALL not allow the origin `null`. The answer to a request from another origin SHALL be the same as today, with no `Access-Control-Allow-Origin`.

#### Scenario: TC-65-03 Origins not on the list

- **GIVEN** `STRATA_CORS_ORIGINS=http://localhost:4321`
- **WHEN** a client sends a preflight and a `GET` of a status from `http://localhost:4322`, and from the origin `null`
- **THEN** no answer has `Access-Control-Allow-Origin`

### Requirement: A browser can read Retry-After

For a request from an allowed origin, the API SHALL send `Access-Control-Allow-Origin` and `Access-Control-Expose-Headers` that names `retry-after`. So a script of the origin can read the interval of a status that is not final.

#### Scenario: TC-65-04 The API exposes Retry-After

- **GIVEN** `STRATA_CORS_ORIGINS=http://localhost:4321`
- **WHEN** an account sends a request from the allowed origin for the status of a revision that is not final
- **THEN** the answer has `Retry-After`
- **AND** it has `Access-Control-Expose-Headers` that names `retry-after`

### Requirement: A bad origin stops the start

Each item of `STRATA_CORS_ORIGINS` SHALL be an exact origin: the scheme `http` or `https`, a host, and an optional port. Spaces around an item SHALL not count. A bad item SHALL stop `strata api` at start with a message that names `STRATA_CORS_ORIGINS`. A bad item has a path, a query, a fragment, user information, a trailing slash or a wildcard. An item with no scheme or another scheme is also bad.

#### Scenario: TC-65-05 Bad origins

- **GIVEN** `STRATA_CORS_ORIGINS` holds `*`, `http://localhost:4321/`, `http://localhost:4321/app`, `localhost:4321` or `ftp://localhost`
- **WHEN** `strata api` starts
- **THEN** it stops with a message that names `STRATA_CORS_ORIGINS`
