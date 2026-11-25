# Spec Delta

## MODIFIED Requirements

### Requirement: The actor claim goes to the log only

If a token has an `act` claim, the API SHALL write the account ID of the caller in the request log. The event SHALL say that the token has an actor. The API SHALL write the event after the identity transaction, when the account ID is known. The log SHALL not contain `act.sub`. The claim SHALL have no other effect. The API SHALL ignore the `scope` claim. (0021, amended on 2026-11-24 in #133.)

#### Scenario: TC-21-13 The actor is in the log, and the response does not change

- **GIVEN** a valid P1 token with an `act` claim
- **WHEN** a client sends it
- **THEN** the request log contains the account ID of the caller and no `act.sub`
- **AND** the response is equal to the response for the same token without `act`
