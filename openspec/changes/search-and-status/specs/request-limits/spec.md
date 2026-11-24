# Spec Delta

## Purpose

Keeps each API process inside its bounds (rules S3 and S4 of tech design 0002). A process rejects work above a bound at once, with a defined response. The status routes have a read limit for each revision and for each account (tech design 0047).

## ADDED Requirements

### Requirement: One API process handles at most 256 requests at the same time

One `strata api` process SHALL handle at most 256 requests at the same time (0049). A request above the bound SHALL get `503` with `Retry-After: 1` and no body, at once, with no wait. The API SHALL log each rejection at the bound.

#### Scenario: A request above the bound

- **GIVEN** an API with a bound that the test sets to 1, and one request that the API handles
- **WHEN** a second request arrives
- **THEN** the API answers it `503` with `Retry-After: 1` at once
- **WHEN** the first request ends and a third request arrives
- **THEN** the API handles the third request

### Requirement: A token bucket for each revision

Each API process SHALL keep in memory one token bucket for each pair (account, revision). The two status routes SHALL use the same buckets. A bucket SHALL hold at most 3 tokens, and SHALL get 1 token for each `Retry-After` interval. A read of one revision SHALL take 1 token from its bucket. A batch request SHALL take 1 token from the bucket of each revision that it names.

#### Scenario: TC-47-05 Reads above the limit of one revision

- **GIVEN** an account that read the status of revision R 3 times in less than one `Retry-After` interval
- **WHEN** it reads R again
- **THEN** the API answers `429` with `Retry-After`
- **WHEN** it reads a different revision
- **THEN** the API answers with its status
- **PENDING** Task 4.1 adds the test.

#### Scenario: TC-47-12 A backend that waits for 300 revisions

- **GIVEN** one account that waits for 300 revisions
- **WHEN** it sends 3 batch requests of 100 revisions in each `Retry-After` interval, for 10 intervals
- **THEN** no response is `429`
- **PENDING** Task 4.1 adds the test.

### Requirement: An empty bucket stops the complete request

When one bucket of a status request is empty, the API SHALL answer `429` with `Retry-After` for the complete request, and SHALL take no token from the other buckets.

#### Scenario: A batch with one empty bucket

- **GIVEN** an account whose bucket of revision R is empty, and a full bucket of revision S
- **WHEN** it sends a batch request that names R and S
- **THEN** the API answers `429`, and the bucket of S keeps its tokens

### Requirement: A token bucket for each account

Each account SHALL also have one bucket for all its status requests. The bucket SHALL hold at most 20 tokens and SHALL get 10 tokens each second. Each status request SHALL take 1 token from it. An empty bucket SHALL give `429` with `Retry-After`.

#### Scenario: TC-47-14 Many requests for different revisions

- **GIVEN** an account that sends 21 status requests in less than one second, each for a different revision
- **WHEN** it sends one more
- **THEN** the API answers `429`
- **PENDING** Task 4.1 adds the test.

### Requirement: The count of buckets has a bound

One API process SHALL keep at most 100,000 buckets (0049). At the bound, it SHALL remove the bucket that it used least recently before it adds a new one. A removed bucket SHALL count as a full bucket on its next use.

#### Scenario: The bucket used least recently goes first

- **GIVEN** an API with a bound of 2 buckets, and the buckets of revisions R1 and R2, with R1 used last
- **WHEN** the account reads revision R3
- **THEN** the bucket of R2 goes, and the buckets of R1 and R3 stay
