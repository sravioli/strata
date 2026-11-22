# Spec Delta

## Purpose

Finds the tenant and the account of each API request from a token that a trusted external issuer signed. The identity transaction then reads the statuses before the decision of 0004 (tech designs 0003, 0004 and 0021).

## ADDED Requirements

### Requirement: A token failure gets one response

The API SHALL do steps 1 to 10 of 0021 for each request, in that order. Each failure of steps 1 to 9 SHALL stop the request with `401`, the header `WWW-Authenticate: Bearer` and an empty body. The response SHALL not show which step failed.

#### Scenario: TC-21-14 Each rejection of a token gives the same response

- **WHEN** a test sends one request that fails at each step that rejects a token
- **THEN** the status code and the body of the response are equal
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-21-16 A large token is rejected before the parse

- **GIVEN** a token larger than 8 KiB
- **WHEN** a client sends it
- **THEN** the API rejects the request before it parses the token
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-21-03 A token of an issuer that is not registered fails

- **GIVEN** a token signed by a key pair that is not registered, with an `iss` that is not registered
- **WHEN** a client sends it
- **THEN** the API rejects the request
- **PENDING** change 1 of #49 adds the test

### Requirement: The signature uses the algorithm of the issuer only

The API SHALL verify the signature with the algorithm of the row of the issuer only. It SHALL reject a token whose header names a different algorithm. A key SHALL come only from the JWKS URL of the issuer row. The API SHALL ignore the headers `jku`, `jwk`, `x5u` and `x5c`.

#### Scenario: TC-21-04 A different algorithm fails

- **GIVEN** issuer P1 with algorithm ES256
- **WHEN** a client sends a P1 token with `alg` = `none`, with HS256 signed with the public key of P1, or with RS256
- **THEN** the API rejects each request
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-21-12 A key in the header is not used

- **GIVEN** a P1 token signed by an attacker key, with a `jku`, `jwk`, `x5u` or `x5c` header that gives that key
- **WHEN** a client sends it
- **THEN** the API rejects the request and makes no request to the URL
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-3-13 A changed or expired token runs no tenant query

- **GIVEN** an access token of an account of A
- **WHEN** a client changes the tenant in the token, breaks its signature, names a different algorithm or uses it after its expiry time
- **THEN** the API rejects the request and runs no query on a tenant-owned table
- **PENDING** change 1 of #49 adds the test

### Requirement: The API verifies the claims

The API SHALL verify the claims of step 6 of 0021. They are `iss`, `aud`, `exp`, `iat` and `nbf`, with 60 seconds of tolerance, and `exp` minus `iat` against `max_lifetime_s`. The tenant claim and the account claim SHALL each be one string of 1 to 256 bytes.

#### Scenario: TC-21-05 Claims of time and audience

- **GIVEN** a valid P1 token
- **WHEN** the client changes `aud` or removes `exp` or `iat`
- **THEN** the API rejects each request
- **WHEN** `exp` is more than 60 seconds in the past, or `nbf` or `iat` is more than 60 seconds in the future
- **THEN** the API rejects each request
- **WHEN** `exp` minus `iat` is above `max_lifetime_s`
- **THEN** the API rejects the request
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-21-06 Claims of tenant and account

- **GIVEN** a valid P1 token
- **WHEN** the tenant claim or the account claim is missing, empty, an array, a number, or longer than 256 bytes
- **THEN** the API rejects each request
- **PENDING** change 1 of #49 adds the test

### Requirement: The tenant comes from the binding of the issuer

The API SHALL find the tenant only through the binding of the issuer and the tenant value of the token. It SHALL not read a tenant identifier from a header, a query parameter, the path or the body. The text of a request SHALL go to PostgreSQL only as a parameter.

#### Scenario: TC-21-01 A bound tenant value gives its tenant

- **GIVEN** issuer P1 with a binding from `acme-corp` to tenant A
- **WHEN** a client sends a valid P1 token with tenant value `acme-corp`
- **THEN** the API acts for tenant A
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-21-02 A binding of a different issuer does not apply

- **GIVEN** issuers P1 and P2, and a binding of P2 from `t-2` to tenant B
- **WHEN** a client sends a valid P1 token with tenant value `t-2`
- **THEN** the API rejects the request
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-21-15 A runtime role cannot list the bindings

- **GIVEN** bindings for issuers P1 and P2
- **WHEN** a runtime role selects from `issuer_bindings` with no settings, or with only `app.issuer_id`
- **THEN** the result is empty
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-3-11 A tenant in the request is ignored

- **GIVEN** an access token of an account of A
- **WHEN** a request carries the `tenant_id` of B in a header, in a query parameter, in the path or in the body
- **THEN** the API acts for A in each case
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-3-16 SQL text in a parameter stays data

- **GIVEN** an access token of an account of A
- **WHEN** a string parameter of a request contains SQL text that sets the tenant context to B
- **THEN** the API uses the text as data
- **AND** the tenant context stays A
- **PENDING** change 1 of #49 adds the test

### Requirement: The first request creates the account

In the identity transaction, the API SHALL read the account for `(tenant_id, issuer_id, subject)`. If none exists, it SHALL insert it as active, with `on conflict do nothing`, and read it again.

#### Scenario: TC-21-07 Two first requests give one account

- **GIVEN** no account for (P1, `u-1`) in tenant A
- **WHEN** two requests with valid tokens for (P1, `u-1`) arrive at the same time
- **THEN** the two requests succeed, and tenant A has exactly one account for (P1, `u-1`)
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-21-08 One subject of two issuers gives two accounts

- **GIVEN** issuers P1 and P2 bound to tenant A
- **WHEN** P1 and P2 tokens with the same subject arrive
- **THEN** tenant A has two different accounts
- **PENDING** change 1 of #49 adds the test

### Requirement: A status that is not active stops the request

The API SHALL read the status of the issuer, the binding, the tenant and the account on each request, with no cache. An issuer that is not active, or a binding that does not exist, SHALL give the response of a token failure. A tenant or an account that is not active SHALL give `403` with an empty body. After the rejection, the API SHALL run no further query.

#### Scenario: TC-21-09 A change by the operator applies to the next request

- **GIVEN** a valid P1 token
- **WHEN** an operator sets P1 to not active, or removes the binding of its tenant value
- **THEN** the next request with that token fails
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-3-09 A tenant that is not active

- **GIVEN** a tenant that is not active
- **WHEN** one of its accounts sends a request
- **THEN** the API rejects the request and runs no further query
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-4-04 An account that the operator stops

- **GIVEN** an active account with an access token that is not expired
- **WHEN** an operator sets the account to not active, and the account sends a request
- **THEN** the API rejects the request
- **PENDING** change 1 of #49 adds the test

### Requirement: The key cache of each issuer

Each API process SHALL keep the keys of each issuer in memory, with the rules of "Signing keys" in 0021:

- HTTPS, or HTTP only for a loopback address, with a 5-second timeout, a 64 KiB limit and no redirect.
- A fetch each 10 minutes, and at most one more fetch each minute for an unknown `kid`.
- Only keys that agree with the algorithm of the issuer.

#### Scenario: TC-21-10 Unknown key identifiers cause at most one fetch

- **GIVEN** keys in the cache of P1
- **WHEN** 100 tokens with unknown `kid` values arrive in one minute
- **THEN** the API fetches the JWKS of P1 at most one time in that minute
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-21-11 A failed fetch keeps the keys

- **GIVEN** keys in the cache of P1
- **WHEN** the JWKS URL of P1 stops responding
- **THEN** the API accepts valid P1 tokens with the keys that it has
- **GIVEN** an issuer whose keys the API never fetched
- **THEN** the API rejects all its tokens
- **PENDING** change 1 of #49 adds the test

#### Scenario: TC-21-18 The development issuer works end to end

- **GIVEN** `strata-dev` serves its JWKS on a loopback address, and the development issuer is registered
- **WHEN** a client requests a token from `strata-dev` and calls the public API
- **THEN** the API fetches the keys over HTTP and accepts the request
- **PENDING** L2 test, before a release. Change 4 of #49 adds `make dev`.

### Requirement: The actor claim goes to the log only

If a token has an `act` claim, the API SHALL write `act.sub`, cut to 256 bytes, in the request log. The claim SHALL have no other effect. The API SHALL ignore the `scope` claim.

#### Scenario: TC-21-13 The actor is in the log, and the response does not change

- **GIVEN** a valid P1 token with an `act` claim
- **WHEN** a client sends it
- **THEN** the request log contains `act.sub`, and the response is equal to the response for the same token without `act`
- **PENDING** change 1 of #49 adds the test

### Requirement: A full pool gives 503

The pool of each API process SHALL hold 20 connections. A request SHALL wait at most 2 seconds for a connection. Then the API SHALL answer `503` with `Retry-After: 1`.

#### Scenario: TC-2-06 The wait for a connection has a limit

- **GIVEN** a database pool that is full, and a request that waits for a connection
- **WHEN** the wait reaches its time limit
- **THEN** the API answers 503 with `Retry-After`, and it does not wait longer
- **PENDING** change 1 of #49 adds the test
