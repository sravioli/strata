---
status: "accepted"
date: 2026-11-20
decision-makers: sravioli
tech-design: docs/tech-design/0021-identity-providers.md
---

# Strata verifies the tokens of trusted external issuers, and each issuer can name only its bound tenants

## Context and Problem Statement

The tenant of a request comes only from a verified access token. Strata must accept the tokens of several identity providers at the same time. Which providers issue the tokens that Strata accepts, and how does Strata trust each one? The issue is [#21](https://github.com/sravioli/strata/issues/21).

## Considered Options

* Verify the tokens of trusted external issuers only, and add a Strata issuer later if necessary
* Strata issues its own tokens with password login, and also verifies external tokens
* The service token of a product, with a header that names the user
* Issuers in a configuration file
* A Strata `tenant_id` claim in each token

## Decision Outcome

Chosen option: "Verify the tokens of trusted external issuers only". The products of the owner already have identity providers. A Strata issuer needs password storage, login limits and key rotation, and no known client needs them in v1.

The decision has these parts:

* Strata issues no tokens in v1. A later Strata issuer is one more trusted issuer. The verification steps do not change.
* An operator registers each trusted issuer in the database. The row holds the issuer, the JWKS URL, the audience, one asymmetric algorithm, the tenant claim, the account claim and the longest token lifetime.
* An issuer binding maps one tenant value of one issuer to one Strata tenant. A token whose tenant value has no binding for its issuer has no access.
* The user always comes from a signed claim: the token of the user, or a token from token exchange (RFC 8693). Strata never reads a user from a header.
* An account is the pair (issuer, subject) in one tenant. The API creates it on the first valid request. The client token of a product is also an account.
* The API accepts a token lifetime of 1 hour at most. Clock tolerance is 60 seconds.
* The API fetches the keys from the JWKS URL and keeps them in memory. Issuer rows and bindings have no cache, so a change has an effect on the next request.
* Scopes come later. They can only make the rights of an account smaller. An operator can change the rights of an account.
* A separate `strata-dev` tool issues tokens locally and in tests. The server binary cannot issue tokens.

### Consequences

* Good, because Strata stores no passwords and keeps no signing keys of its own.
* Good, because a compromised issuer reaches only the tenants that it has bindings for.
* Good, because an operator can stop an issuer, a binding or an account, and the next request fails.
* Bad, because the login of each user depends on its provider.
* Bad, because a product with no identity provider cannot use Strata in v1.
* Bad, because a stolen token works until it expires, up to 1 hour, unless an operator stops its account.
* Bad, because the key cache is an exception to S1 of ADR 0003. A copy can accept a removed key for up to 10 minutes (Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85))).
* Not decided: the JWT library, and the claim that Duende puts in its client tokens for the subject.
