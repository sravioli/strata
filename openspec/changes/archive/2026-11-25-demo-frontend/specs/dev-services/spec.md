# Spec Delta

## ADDED Requirements

### Requirement: Keycloak is a compose profile

`compose.yaml` SHALL have the service `keycloak` in the profile `frontend`. `docker compose up --wait` without the profile, `make l1` and `make dev` SHALL not start it. Its image SHALL be pinned by tag and digest. It SHALL listen on `127.0.0.1:8180`. Its memory limit SHALL be 1.5 GiB.

#### Scenario: The default start leaves Keycloak out

- **WHEN** a developer runs `docker compose up --wait` with no profile
- **THEN** no service of the profile `frontend` starts

### Requirement: Keycloak gives the same issuer and realm at each start

Keycloak SHALL have the fixed hostname `http://localhost:8180`, so the issuer of its tokens is always `http://localhost:8180/realms/strata`. It SHALL keep no data in a volume. A new container SHALL import the realm from `dev/keycloak/strata-realm.json`.

#### Scenario: Keycloak issues tokens with the claims of Strata

- **GIVEN** the profile `frontend` runs
- **WHEN** `alice@acme` gets an access token from the realm `strata`
- **THEN** its `iss` is `http://localhost:8180/realms/strata`, its `aud` holds `strata`, and its claim `tenant` is the string `acme`
- **AND** a new container gives the same `sub` for `alice@acme`

### Requirement: make dev allows the origin of the site

When the variable `CI` is not in the environment and `STRATA_CORS_ORIGINS` is not in the environment, `make dev` SHALL start `strata api` with `STRATA_CORS_ORIGINS=http://localhost:4321`. A value in the environment SHALL win. When `CI` is in the environment, `make dev` and `make l2` SHALL set no value.

#### Scenario: A local run allows the site

- **GIVEN** the variable `CI` is not set
- **WHEN** a developer runs `make dev`
- **THEN** the API answers a preflight from `http://localhost:4321`

#### Scenario: A run in CI has no CORS

- **GIVEN** the variable `CI` is in the environment
- **WHEN** the `l2` job of CI runs `make l2`
- **THEN** `STRATA_CORS_ORIGINS` is not set for the API

### Requirement: One command starts the frontend

`make web` SHALL start the profile `frontend` and wait until Keycloak answers. It SHALL register the issuer of Keycloak with the algorithm `RS256`, the audience `strata` and the tenant claim `tenant`, and bind the tenant values `acme` and `globex`. Then it SHALL run the site on `http://localhost:4321`. A second run SHALL not fail on rows that exist. `make web` SHALL need the processes of `make dev`, and SHALL stop with a message when the API does not answer.

#### Scenario: make web with no API

- **GIVEN** `make dev` does not run
- **WHEN** the developer runs `make web`
- **THEN** it stops with a message that names `make dev`

### Requirement: The bucket allows the upload from the site

`make web` SHALL apply the CORS rule of `dev/garage-cors.xml` to the bucket `strata`: the origin `http://localhost:4321`, the method `PUT` and the header `content-type`.

#### Scenario: The first run of make web

- **GIVEN** `make dev` runs
- **WHEN** the developer runs `make web`
- **THEN** the site answers on `http://localhost:4321`
- **AND** a browser on the site can do the PUT of an upload to Garage
