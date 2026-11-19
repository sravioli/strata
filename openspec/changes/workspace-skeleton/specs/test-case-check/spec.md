# Spec Delta

## Purpose

Makes sure that no test case is silently dropped. Each test case ID that a tech design or an OpenSpec spec defines has an automated test that carries the ID, or a `pending` marker.

## ADDED Requirements

### Requirement: A spec scenario has a test or a pending marker

The name of a scenario in an OpenSpec spec can contain a test case ID. The check SHALL fail for that ID when no automated test carries it and the scenario has no `pending` marker. The failure message SHALL name the ID and the spec file. The specs of active changes and the main specs both count. Archived changes do not count.

#### Scenario: Scenario with a test passes

- **GIVEN** a spec scenario with the ID `TC-9-02` in its name
- **WHEN** a test file carries `TC-9-02` and the check runs
- **THEN** the check passes for `TC-9-02`

#### Scenario: Scenario with a pending marker passes

- **GIVEN** a spec scenario with the ID `TC-9-02` in its name and a `pending` marker
- **WHEN** no test file carries `TC-9-02` and the check runs
- **THEN** the check passes for `TC-9-02`

#### Scenario: Scenario with no test and no marker fails

- **GIVEN** a spec scenario with the ID `TC-9-02` in its name and no `pending` marker
- **WHEN** no test file carries `TC-9-02` and the check runs
- **THEN** the check fails, and the message names `TC-9-02` and the spec file

### Requirement: A spec is the source of truth for an ID

When a spec scenario contains a test case ID, the check SHALL use only the spec for that ID. The check SHALL ignore the "Automated test" column of the tech design for that ID. The check SHALL not require a change to an accepted tech design.

#### Scenario: Design row says pending, spec has no marker and no test

- **GIVEN** a tech design row for `TC-9-02` that says `pending`, and a spec scenario for `TC-9-02` with no `pending` marker
- **WHEN** no test file carries `TC-9-02` and the check runs
- **THEN** the check fails for `TC-9-02`

### Requirement: An ID only in a tech design passes while its row says pending

A test case ID can be in the test case table of a tech design and in no spec scenario. Then the check SHALL pass for that ID if the "Automated test" cell of the row says `pending`. If the cell says a different value, the check SHALL require a test that carries the ID. The template files of `docs/tech-design/` do not count.

#### Scenario: Design-only ID with pending passes

- **GIVEN** a tech design row for `TC-5-01` that says `pending`, and no spec scenario for `TC-5-01`
- **WHEN** no test file carries `TC-5-01` and the check runs
- **THEN** the check passes for `TC-5-01`

#### Scenario: Design-only ID with no pending and no test fails

- **GIVEN** a tech design row for `TC-5-01` whose "Automated test" cell is not `pending`, and no spec scenario for `TC-5-01`
- **WHEN** no test file carries `TC-5-01` and the check runs
- **THEN** the check fails, and the message names `TC-5-01` and the tech design file

### Requirement: A test carries only defined IDs

The check SHALL fail when a test file carries a test case ID that no tech design and no spec defines. The failure message SHALL name the ID and the test file. A test file is a tracked file that is not Markdown, outside `docs/` and `openspec/`. The unit tests of the check and each path with a `fixtures` folder do not count.

#### Scenario: Test with an unknown ID fails

- **GIVEN** a test file that carries `TC-99-01`
- **WHEN** no tech design and no spec defines `TC-99-01` and the check runs
- **THEN** the check fails, and the message names `TC-99-01` and the test file

### Requirement: CI runs the check

CI SHALL run the check on each run, because a change to any file can add or remove an ID. A failure of the check SHALL make the required CI result fail.

#### Scenario: A failure blocks the merge

- **GIVEN** a pull request that adds a spec scenario with a new ID and no test and no `pending` marker
- **WHEN** CI runs
- **THEN** the job of the check fails, and the required CI result fails
