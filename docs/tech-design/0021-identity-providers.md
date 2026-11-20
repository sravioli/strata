- Issue: [#21](https://github.com/sravioli/strata/issues/21)
- Status: accepted
- Start date: 2026-11-20
- Decided by: sravioli, 2026-11-20

# Identity providers: several trusted issuers, each bound to its tenants

## Summary

In v1, Strata issues no access tokens. It verifies the access tokens of external identity providers. An operator registers each trusted issuer in the database and binds the issuer to the tenants that it can name. The API maps the tenant value in a verified token to a Strata tenant through that binding. An account is the pair (issuer, subject). The API creates the account on the first valid request. The API fetches the signing keys of each issuer from its JWKS URL and keeps them in memory. A separate development tool issues tokens for tests and for a laptop. The server binary cannot issue tokens.

## Motivation

The tenancy design ([#3](https://github.com/sravioli/strata/issues/3)) says that the tenant of a request comes only from a verified access token. It left open who issues that token. The v1 authorization design ([#4](https://github.com/sravioli/strata/issues/4)) builds the principal from the verified token. The walking skeleton ([#49](https://github.com/sravioli/strata/issues/49)) needs a token that the API accepts.

The owner gave these facts on 2026-11-18:

- Strata must accept the tokens of several identity providers at the same time. Example: one product uses Duende IdentityServer, and a different product uses a different library.
- A product calls Strata for a user, and the request must name that user.

The products of the owner already have identity providers. If Strata also issues tokens, it must store passwords, limit login attempts and rotate its own signing keys. No known client needs that in v1.

## Guide-level explanation

### Terms

- **Trusted issuer**: an identity provider that an operator registered in Strata. Strata accepts only the tokens of trusted issuers.
- **Issuer binding**: a row that maps one tenant value of one trusted issuer to one Strata tenant. An issuer can name only the tenants that it has a binding for.
- **Tenant value**: the value of the tenant claim in a token. The provider sets it. It is not a Strata `tenant_id`.
- **Subject**: the value of the account claim in a token. It identifies one user or one client of the provider.
- **Development issuer**: the issuer of the `strata-dev` tool. It exists only on a laptop and in tests.

### Example: two products

Product P1 uses Duende IdentityServer at `https://id.p1.example`. Product P2 uses a different provider at `https://auth.p2.example`. Strata has the tenants Acme and Birch.

An operator does these steps:

1. The operator registers P1 as a trusted issuer. The tenant claim of P1 is `org`.
2. The operator binds the P1 tenant value `acme-corp` to the tenant Acme.
3. The operator registers P2 as a trusted issuer. The tenant claim of P2 is `tenant`.
4. The operator binds the P2 tenant value `t-0042` to the tenant Birch.

Alice signs in to P1. P1 gives her an access token with `iss` = `https://id.p1.example`, `sub` = `u-17` and `org` = `acme-corp`. The P1 backend sends this token to Strata. Strata verifies the token and finds the binding. The request acts for the account (P1, `u-17`) in Acme.

A token of P1 with `org` = `t-0042` has no access. P1 has no binding for `t-0042`. The binding of `t-0042` belongs to P2 only.

### Ways a product can name a user

A product can name a user in two ways. Strata accepts the two ways and reads the same claims in each case.

1. **The user's own token.** The provider issues a token for the user with Strata as its audience. The product sends that token to Strata.
2. **Token exchange.** The product exchanges its own token and the token of the user for one new token (RFC 8693). The new token names the user in `sub` and the product in the `act` claim.

Strata never reads a user from a header or from a different part of the request. A product that sends only its own token acts as itself. Strata treats such a client as one more account.

### What an operator does

The operator command has these tasks:

- Register a trusted issuer, change its settings, and set it to active or not active.
- Add or remove an issuer binding.
- Set an account to active or not active. The account exists after its first request.

A change has an effect on the next request. The API does not keep issuer rows or bindings in memory.

### What a developer does

The `strata-dev` tool makes a key pair and serves its JWKS on a loopback address. It issues tokens for a tenant value and a subject:

```sh
strata-dev token --tenant acme --sub alice
```

The local setup registers the development issuer and its bindings. The L1 tests use the same code in the test process.

## Reference-level explanation

### Data model

These tables support the design. They follow the rules of the tenancy design ([#3](https://github.com/sravioli/strata/issues/3)).

**`issuers`** is a global table. It holds settings, not tenant content.

| Column | Meaning |
|---|---|
| `issuer_id` | UUID. Primary key. |
| `issuer` | The exact `iss` value. Unique. |
| `jwks_url` | The URL of the signing keys. |
| `audience` | The value that `aud` must contain. |
| `algorithm` | One of `RS256`, `ES256` and `EdDSA`. |
| `tenant_claim` | The name of the claim that holds the tenant value. |
| `account_claim` | The name of the claim that holds the subject. The default is `sub`. |
| `max_lifetime_s` | The longest token lifetime that the API accepts, in seconds. The default is 3600. A constraint keeps it between 1 and 3600. |
| `status` | Active or not active. |

**`issuer_bindings`** maps a tenant value to a tenant. Its primary key is `(issuer_id, tenant_value)`. It has a foreign key to `issuers` and a foreign key to `tenants`. Each tenant can have bindings from more than one issuer.

This table has row-level security enabled and forced. It has exactly one policy, for `select` only:

```sql
using (
  issuer_id = nullif(current_setting('app.issuer_id', true), '')::uuid
  and tenant_value = nullif(current_setting('app.tenant_value', true), '')
)
```

A runtime role thus reads a binding only when the transaction gives the exact issuer and the exact tenant value. A runtime role cannot list bindings, so it cannot list the tenants of an issuer. Only the operator role adds or removes a binding.

**`accounts`** changes. The columns `issuer_id` and `subject` replace `username`. The triple `(tenant_id, issuer_id, subject)` is unique. Thus the same subject from two issuers gives two accounts. The same subject of one issuer in two tenants also gives two accounts, as the glossary requires.

### Verification of a token

The API does these steps for each request. Each step that fails stops the request. The API gives the same response for each failure. That response does not show which step failed.

1. The API rejects a token that is larger than 8 KiB.
2. The API reads `iss` and `kid` from the token. These values are not verified yet. If `kid` is missing, the API rejects the token.
3. The API reads the row of `issuers` for `iss` in one statement, with no open transaction. If no row exists, or the issuer is not active, the API rejects the token.
4. The API selects the key for `kid` in the key cache of that issuer. See "Signing keys".
5. The API verifies the signature with the algorithm of the issuer row only. It rejects a token whose header names a different algorithm. Thus `none` and all HS* algorithms always fail.
6. The API verifies the claims:
   - `iss` is equal to the issuer of the row, character for character.
   - `aud` contains the audience of the row.
   - `exp` exists. The current time is not later than `exp` plus 60 seconds.
   - `iat` exists. `iat` is not later than the current time plus 60 seconds.
   - `exp` minus `iat` is not more than `max_lifetime_s`.
   - If `nbf` exists, the current time is not earlier than `nbf` minus 60 seconds.
   - The tenant claim exists. It is one string of 1 to 256 bytes.
   - The account claim exists. It is one string of 1 to 256 bytes.
7. The API starts a transaction. It sets `app.issuer_id` and `app.tenant_value` with `set_config(..., true)` and reads the binding. If no binding exists, the API rejects the token.
8. The API opens the tenant context for the `tenant_id` of the binding, in the same transaction.
9. The API reads the account for `(tenant_id, issuer_id, subject)`. If no account exists, the API inserts it as active. The insert uses `on conflict do nothing` and then reads the row again. Thus two first requests at the same time give one account.
10. The API reads the status of the tenant and of the account, as the tenancy design says. The v1 authorization design ([#4](https://github.com/sravioli/strata/issues/4)) then decides the request.

Steps 4 to 6 hold no database connection. Step 4 can wait for a fetch of keys, so a token with an unknown `kid` must not hold a connection of the pool. Steps 3 and 7 read the database at different times. If the operator changes the issuer between them, the next request uses the change.

The API ignores the `jku`, `jwk`, `x5u` and `x5c` headers. A key comes only from the JWKS URL of the issuer row.

Step 3 uses a value that the API did not verify. The API uses that value only to select the issuer row. All access comes from the claims after step 5.

The API ignores the `scope` claim in v1. See "Scopes".

If the token has an `act` claim, the API writes `act.sub` in the request log. The value is cut to 256 bytes. The `act` claim has no other effect in v1.

### Signing keys

Each API instance keeps a key cache for each issuer in memory:

- The API fetches the JWKS from `jwks_url` with HTTPS. The URL can use HTTP only for a loopback address. The development issuer uses that case.
- A fetch has a 5-second timeout and a 64 KiB size limit. It follows no redirect.
- The API keeps only the keys whose type agrees with the algorithm of the issuer. If a key has `alg`, it must be equal to that algorithm. If a key has `use`, it must be `sig`.
- The API fetches the keys of each issuer again every 10 minutes.
- If a token names a `kid` that the cache does not have, the API fetches the keys again. It does this at most one time per minute for each issuer. When that limit applies, the API rejects the token.
- If a fetch fails, the API keeps the last keys that it fetched.
- If the API never fetched keys for an issuer, it rejects all tokens of that issuer.

A provider that removes a key from its JWKS stops that key in Strata after the next fetch. This takes 10 minutes at most.

### Scopes

v1 has no scopes. The owner fixed this rule on 2026-11-20 for the later design:

- An action is allowed only if the rights of the account allow it and the scopes of the token allow it.
- Thus the scopes of a token can only make the rights smaller. A token can never have more rights than its account.
- An operator can change the rights of an account.

At that time, the `authorize` function of the v1 authorization design also verifies the scopes. Its theorems then have one more statement: a token with fewer scopes never allows more actions.

### Development issuer

`strata-dev` is a separate binary in its own crate. The server crate does not depend on it. The production image does not contain it. The tool can:

- make an Ed25519 key pair and keep it in a local file,
- serve the JWKS on `127.0.0.1`,
- issue a token for a tenant value, a subject and a lifetime,
- print the operator commands that register the development issuer.

The L1 tests use the token functions of this crate in the test process.

### Examples again

- Alice of P1 (`org` = `acme-corp`): steps 1 to 10 pass. The API creates the account (P1, `u-17`) in Acme on her first request.
- A token of P1 with `org` = `t-0042`: step 7 finds no binding. The API rejects it.
- The operator sets P1 to not active: step 3 rejects the next request of each P1 token.
- P1 rotates its key: the first token with the new `kid` causes one fetch. The API then accepts the token.
- The P1 backend sends its own client token with `sub` = `p1-backend`: the API creates the account (P1, `p1-backend`) in Acme.

## Proof obligations

N/A. This design parses tokens, verifies signatures with a library and reads the database. A Lean model of these steps does not show that the library or PostgreSQL acts correctly. The L0 and L1 test cases below cover this behaviour.

The later scope rule adds one theorem to the v1 authorization design. That design owns it.

## Impacts

### Infrastructure

- No new service and no added cost.
- Each API instance makes outbound HTTPS requests to the JWKS URL of each issuer. The network must permit them.
- The request flow adds two reads by primary key to the status read: the issuer row and the binding. The first request of an account also adds one insert. These statements go to the primary, in the transaction that the tenancy design already opens. The capacity design ([#2](https://github.com/sravioli/strata/issues/2)) sizes the primary for the status read on each request. The two added reads increase that load. A measurement in that design's L3 test cases shows if this is a problem.
- Metrics:
  - the count of rejected tokens, for each reason and each issuer,
  - the count of JWKS fetches and of failed fetches,
  - the age of the keys in each cache.
- The reason for a rejection is in the metric only, never in the response.
- Alert: the keys of an active issuer are older than 30 minutes.
- Rollout: this design is part of the first schema. There is no earlier state to roll back to.

### Security and tenant isolation

| Attack | How the design stops it |
|---|---|
| A token with `alg` = `none`, or HS256 that uses a public key as the secret | The API uses only the algorithm of the issuer row. That algorithm is always asymmetric. |
| A token of a trusted issuer that names a tenant of a different issuer | The issuer has no binding for that tenant value. |
| A token with `jku` or `jwk` that points to the keys of an attacker | The API ignores these headers. |
| A token from a provider that is not registered | No issuer row exists. |
| An ID token or a token for a different service | `aud` does not contain the audience of Strata. |
| A stolen token | The token expires in `max_lifetime_s` at most. An operator can set the account, the issuer or the tenant to not active. The next request then fails. |
| A product that names any user with a header | The API reads users only from signed claims. |
| Many tokens with unknown `kid` values | The API fetches keys at most one time per minute for each issuer. |
| A large or malformed token | The size limit applies before parsing. Step 6 limits the length of each string. |

What a compromised issuer reaches: a provider that an attacker controls can issue tokens for each tenant that it has a binding for. It reaches no other tenant. The operator removes its bindings or sets it to not active. The next request fails.

New untrusted input: the access token, and the JWKS response of each provider.

Tenant names: no step reads a tenant name. A tenant value is the value of the provider. The `issuers` table is global, so a runtime role can list the issuer URLs. An issuer URL can name a customer of the owner. The table holds no tenant names and no tenant content.

### Data and migration

- New tables `issuers` and `issuer_bindings`.
- `accounts` adds `issuer_id` and `subject` and removes `username`. No data exists yet, so no backfill is necessary.
- The `tenants` table loses its login policy. Its policy permits only the row of the tenant context. See the changes to the tenancy design in this pull request.
- Test case TC-3-06 of the tenancy design classifies the two new tables.

### Developer

- A new crate for `strata-dev`.
- The local setup starts `strata-dev` and registers the development issuer.
- There are no passwords to set up.

## Test cases

L0: the claim rules of step 6, for each claim and each limit, with no database and no network.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-21-01 | L1 | Given issuer P1 with a binding from `acme-corp` to tenant A. When a client sends a valid P1 token with tenant value `acme-corp`. Then the API acts for tenant A. | sravioli, 2026-11-20 | pending |
| TC-21-02 | L1 | Given issuers P1 and P2, and a binding of P2 from `t-2` to tenant B. When a client sends a valid P1 token with tenant value `t-2`. Then the API rejects the request. | sravioli, 2026-11-20 | pending |
| TC-21-03 | L1 | Given a token signed by a key pair that is not registered, with an `iss` that is not registered. When a client sends it. Then the API rejects the request. | sravioli, 2026-11-20 | pending |
| TC-21-04 | L1 | Given issuer P1 with algorithm ES256. When a client sends a P1 token with `alg` = `none`, with HS256 signed with the public key of P1, or with RS256. Then the API rejects each request. | sravioli, 2026-11-20 | pending |
| TC-21-05 | L1 | Given a valid P1 token. When the client changes `aud` or removes `exp` or `iat`. Then the API rejects each request. When `exp` is more than 60 seconds in the past, or `nbf` or `iat` is more than 60 seconds in the future. Then the API rejects each request. When `exp` minus `iat` is above `max_lifetime_s`. Then the API rejects the request. | sravioli, 2026-11-20 | pending |
| TC-21-06 | L1 | Given a valid P1 token. When the tenant claim or the account claim is missing, empty, an array, a number, or longer than 256 bytes. Then the API rejects each request. | sravioli, 2026-11-20 | pending |
| TC-21-07 | L1 | Given no account for (P1, `u-1`) in tenant A. When two requests with valid tokens for (P1, `u-1`) arrive at the same time. Then the two requests succeed, and tenant A has exactly one account for (P1, `u-1`). | sravioli, 2026-11-20 | pending |
| TC-21-08 | L1 | Given issuers P1 and P2 bound to tenant A. When P1 and P2 tokens with the same subject arrive. Then tenant A has two different accounts. | sravioli, 2026-11-20 | pending |
| TC-21-09 | L1 | Given a valid P1 token. When an operator sets P1 to not active, or removes the binding of its tenant value. Then the next request with that token fails. | sravioli, 2026-11-20 | pending |
| TC-21-10 | L1 | Given keys in the cache of P1. When 100 tokens with unknown `kid` values arrive in one minute. Then the API fetches the JWKS of P1 at most one time in that minute. | sravioli, 2026-11-20 | pending |
| TC-21-11 | L1 | Given keys in the cache of P1. When the JWKS URL of P1 stops responding. Then the API accepts valid P1 tokens with the keys that it has. Given an issuer whose keys the API never fetched. Then the API rejects all its tokens. | sravioli, 2026-11-20 | pending |
| TC-21-12 | L1 | Given a P1 token signed by an attacker key, with a `jku`, `jwk`, `x5u` or `x5c` header that gives that key. When a client sends it. Then the API rejects the request and makes no request to the URL. | sravioli, 2026-11-20 | pending |
| TC-21-13 | L1 | Given a valid P1 token with an `act` claim. When a client sends it. Then the request log contains `act.sub`, and the response is equal to the response for the same token without `act`. | sravioli, 2026-11-20 | pending |
| TC-21-14 | L1 | For each step that rejects a token. Then the status code and the body of the response are equal. | sravioli, 2026-11-20 | pending |
| TC-21-15 | L1 | Given bindings for issuers P1 and P2. When a runtime role selects from `issuer_bindings` with no settings, or with only `app.issuer_id`. Then the result is empty. | sravioli, 2026-11-20 | pending |
| TC-21-16 | L1 | Given a token larger than 8 KiB. When a client sends it. Then the API rejects the request before it parses the token. | sravioli, 2026-11-20 | pending |
| TC-21-17 | L1 | For the dependency graph of the server crate. Then it does not contain the `strata-dev` crate. The build fails if it does. | sravioli, 2026-11-20 | pending |
| TC-21-18 | L2 | Given `strata-dev` serves its JWKS on a loopback address, and the development issuer is registered. When a client requests a token from `strata-dev` and calls the public API. Then the API fetches the keys over HTTP and accepts the request. | sravioli, 2026-11-20 | pending |

L3: none in this design. The added reads use primary keys in the transaction that already exists. The L3 test cases of the capacity design measure the request flow with these reads.

## Drawbacks

- Strata depends on each provider for login. If a provider stops, its users cannot receive new tokens. Tokens that exist work until they expire.
- An operator must register each provider and each binding. A new customer tenant needs one binding at least.
- A product with no identity provider cannot use Strata in v1.
- A stolen token works until it expires, up to 1 hour. Strata cannot cancel one token. It can only set the account to not active.

## Rationale and alternatives

**Strata issues its own tokens in v1.** This needs password storage, login attempt limits and key rotation. No known client needs it. The owner rejected it for v1 on 2026-11-20. A Strata issuer can come later as one more row in `issuers`. The verification steps do not change.

**A header that names the user, with the service token of the product.** This is simple for a product. A leaked service token can then act as each user of its tenants. It also breaks the rule of the v1 authorization design that the principal comes from the verified token. Rejected.

**Issuers in a configuration file.** This is simpler than database rows. But each change needs a restart, and the bindings must agree with tenants that are database rows. Rejected.

**A Strata `tenant_id` claim in the token.** Then a provider must know the identifiers of Strata, and a binding cannot limit a provider to its own tenants. Rejected.

**Accounts that an operator creates before the first request.** This does not scale to approximately 50,000 accounts that the providers manage. Rejected.

**Keys in the issuer row.** This removes the outbound fetch. But each key rotation of a provider then needs an operator. Rejected.

**A cache of issuer rows and bindings.** This saves reads. But a provider that the operator stopped keeps its access until the cache expires. Rejected.

**A token endpoint in the server behind a flag.** This removes the separate tool. But one wrong flag in production lets each client issue tokens. Rejected.

**A full identity provider, for example Keycloak, in the local setup.** This is more realistic. It is also slow to start and large. A later L3 test can add it if necessary.

## Prior art

- RFC 7519, "JSON Web Token (JWT)": https://www.rfc-editor.org/rfc/rfc7519
- RFC 8725, "JSON Web Token Best Current Practices": https://www.rfc-editor.org/rfc/rfc8725. It tells verifiers to use a fixed algorithm and to verify `iss` and `aud`.
- RFC 9068, "JWT Profile for OAuth 2.0 Access Tokens": https://www.rfc-editor.org/rfc/rfc9068
- RFC 8693, "OAuth 2.0 Token Exchange": https://www.rfc-editor.org/rfc/rfc8693. It defines the `act` claim.
- RFC 7517, "JSON Web Key (JWK)": https://www.rfc-editor.org/rfc/rfc7517

## Unresolved questions

None for the owner. The owner accepted the design and its test cases on 2026-11-20.

Implementation resolves these points:

- The JWT library. It must support RS256, ES256 and EdDSA, and a fixed algorithm for each verification.
- The claim that a client-credentials token of Duende uses for its subject. The account claim of an issuer must identify one user or one client in each type of token of that issuer. RFC 9068 makes `sub` necessary in an access token. Thus a provider must put `sub` in its client tokens, or Strata rejects them. `client_id` is not an account claim: the user tokens of a product also contain it, so all users of the product become one account.
- The exact SQL text of the `issuer_bindings` policy.
- The commands and options of the operator command for issuers and bindings.

## Future possibilities

- Scopes, with the rule in "Scopes".
- A Strata issuer, with password login. Issue [#46](https://github.com/sravioli/strata/issues/46) holds the questions for that case.
- A list of the products that can act for users of an issuer, from the `act` claim.
- A full identity provider in the L3 tests.
