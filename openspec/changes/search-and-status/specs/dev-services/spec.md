# Spec Delta

## ADDED Requirements

### Requirement: One command starts the walking skeleton

`make dev` SHALL start the services, apply the migrations as `strata_owner`, and start `strata-dev serve`. It SHALL create the tenants `acme` and `globex`, register the development issuer, and add one binding for each tenant. Then it SHALL start `strata api` and `strata worker` on the host. The worker SHALL run in a delegated cgroup. A second run SHALL not fail on the tenants, the issuer and the bindings that exist. `make dev` SHALL stop the processes that it started when the developer stops it.

#### Scenario: The first run

- **GIVEN** a developer machine with the services stopped and an empty database
- **WHEN** the developer runs `make dev`
- **THEN** the API answers on `127.0.0.1:8080`, and a token of `strata-dev token --tenant acme --sub alice` gets `200` from a search

#### Scenario: A second run

- **GIVEN** a database where an earlier run of `make dev` created the tenants and the bindings
- **WHEN** the developer runs `make dev` again
- **THEN** it starts the API and the worker, and fails on no step

### Requirement: The L2 tests run against the walking skeleton

`make l2` SHALL start the processes of `make dev` in the background, run the L2 tests over HTTP, and stop the processes, also when a test fails. The L2 tests SHALL be test targets with the feature `l2`, so that `make rust` and `make l1` do not run them. An L2 test SHALL fail when the API does not answer. It SHALL not skip.

#### Scenario: TC-49-01 From upload to search on a laptop

- **GIVEN** the walking skeleton on a laptop, with the tenants A and B and an account of each
- **WHEN** the account of A uploads a PDF with a unique word, and polls the status of the revision until it is final
- **THEN** the status is `ready`
- **AND** a search of A for the word returns the document
- **AND** the same search of B returns no result
- **PENDING** Task 7.2 adds the L2 test.

#### Scenario: The processes stop after a failed test

- **GIVEN** an L2 test that fails
- **WHEN** `make l2` ends
- **THEN** no process of the API, the worker or `strata-dev` that `make l2` started runs

### Requirement: CI runs the L2 tests

The `l2` job of CI SHALL run on each pull request after the `l1` job. It SHALL have the same services, nsjail, delegated cgroup and `pdftotext` as the `l1` job, and SHALL run `make l2`. The job that gates the pull request SHALL need the `l2` job.

#### Scenario: The l2 job runs after l1

- **WHEN** CI runs for a pull request
- **THEN** the `l2` job starts after the `l1` job passes, and runs `make l2`
- **AND** a failed `l2` job fails the gate of the pull request
