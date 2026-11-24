# Spec Delta

## MODIFIED Requirements

### Requirement: A status that is not active stops the request

The API SHALL read the status of the issuer, the binding, the tenant and the account on each request, with no cache. An issuer that is not active, or a binding that does not exist, SHALL give the response of a token failure. A tenant or an account that is not active SHALL give `403` with an empty body. After the rejection, the API SHALL run no further query.

#### Scenario: TC-21-09 A change by the operator applies to the next request

- **GIVEN** a valid P1 token
- **WHEN** an operator sets P1 to not active, or removes the binding of its tenant value
- **THEN** the next request with that token fails

#### Scenario: TC-3-09 A tenant that is not active

- **GIVEN** a tenant that is not active
- **WHEN** one of its accounts sends a request
- **THEN** the API rejects the request and runs no further query

#### Scenario: TC-4-04 An account that the operator stops

- **GIVEN** an active account with an access token that is not expired
- **WHEN** an operator sets the account to not active, and the account sends a request
- **THEN** the API rejects the request

#### Scenario: TC-4-03 An account that is not active, on each account action

- **GIVEN** an account that is not active in an active tenant
- **WHEN** the account sends a request for each account action: a start of an upload, a finish, the status of one revision, the batch status, and a search
- **THEN** the API rejects each request and runs no query on documents
- **PENDING** Task 6.1 adds the test.
