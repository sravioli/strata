# revision-lifecycle Specification

## Purpose
Defines the five internal states of a revision and the one pure function that changes them, with Lean proofs of the rules of 0047 and 0007 (tech design 0049, "The lifecycle of a revision").

## Requirements

### Requirement: A revision has five states

A revision SHALL be in one state: `AwaitingUpload`, `Uploaded`, `Ready`, `Failed` or `Superseded`. A `Failed` revision SHALL hold one class: `FileTooLarge` (0006), or a class of 0007: `ExtractionTimeout`, `ExtractionMemory`, `ExtractionCrash`, `ExtractionOutputTooLarge`, `ArchiveTooLarge`, `UnsupportedFormat`, `EncryptedFile`, `MalformedFile` or `InvalidOutput`. A revision in each other state SHALL hold no class. The database SHALL refuse each other state and each other class.

#### Scenario: A state that is not in the list

- **WHEN** a transaction sets the state of a revision to a value that is not in the list
- **THEN** PostgreSQL rejects the write

### Requirement: The revision state function holds the table of events

One pure function SHALL give the next revision for a revision and an event, with the table of events of 0049. Each pair of a state and an event that the table does not name SHALL leave the revision as it is. The API and the worker SHALL write the result of this function, and SHALL not decide a change in SQL.

#### Scenario: The finish events

- **GIVEN** a revision in `AwaitingUpload`
- **WHEN** the function gets `Finish`
- **THEN** the revision is `Uploaded`
- **WHEN** the function gets `FinishTooLarge`
- **THEN** the revision is `Failed` with `FileTooLarge`

#### Scenario: A successful extraction

- **GIVEN** a revision in `Uploaded`
- **WHEN** the function gets `Extracted(no_text)`
- **THEN** the revision is `Ready`, with that value of `no_text`

#### Scenario: A crash, the time limit or invalid output retries one time

- **GIVEN** a revision in `Uploaded` with no retry mark
- **WHEN** the function gets `ExtractFailed` with `ExtractionCrash`, `ExtractionTimeout` or `InvalidOutput`
- **THEN** the revision stays `Uploaded` and gets the retry mark
- **WHEN** the function gets the same event for the revision with the mark
- **THEN** the revision is `Failed` with that class

#### Scenario: Each other class fails at once

- **GIVEN** a revision in `Uploaded`, with or without the retry mark
- **WHEN** the function gets `ExtractFailed` with a class other than `ExtractionCrash`, `ExtractionTimeout` and `InvalidOutput`
- **THEN** the revision is `Failed` with that class

#### Scenario: A newer revision becomes ready

- **GIVEN** a revision in `AwaitingUpload`, `Uploaded` or `Ready`
- **WHEN** the function gets `NewerReady`
- **THEN** the revision is `Superseded`

#### Scenario: Each pair of a state and an event

- **WHEN** an L0 test applies each event to a revision in each state, with and without the retry mark
- **THEN** each result is the result of the table, and each pair that the table does not name gives the same revision

#### Scenario: The finish writes the result of the function

- **GIVEN** a revision in `AwaitingUpload` with its object
- **WHEN** the client finishes the upload
- **THEN** the state that the database holds is the state that the function gives for `Finish`

### Requirement: Extraction only of an uploaded revision

A second pure function SHALL be true for an `Uploaded` revision only. The worker SHALL use it to decide if it extracts a revision.

#### Scenario: Each state

- **WHEN** an L0 test applies the function to a revision in each state
- **THEN** the result is true for `Uploaded` only

### Requirement: The proofs of the revision state function

The Lean proofs SHALL hold each theorem of the "Proof obligations" table of 0049, with no `sorry` and only the standard axioms:

- `final_sticky`
- `ready_only_superseded`
- `retry_once`
- `extract_only_uploaded`
- `revision_total`

`Strata.revision_step_refines` and `Strata.may_extract_refines` SHALL show that each function agrees with the model in `Strata/Model/Revision.lean`.

#### Scenario: The proof check passes

- **WHEN** `make verify` runs on the pull request
- **THEN** the translation agrees with the Rust code, and each named theorem exists and checks

#### Scenario: A change to the function with no new proof fails

- **WHEN** the Rust function changes and the Lean translation does not
- **THEN** `make verify` fails
