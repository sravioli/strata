# Spec Delta

## Purpose

Makes sure that each theorem that Strata calls "proved" is about the Rust code that runs, has no unfinished proof, and depends on no assumption. These are the checks of tech design 0009.

## ADDED Requirements

### Requirement: Fresh translation

The `verify` job SHALL translate the core crate with the pinned Charon and Aeneas, and compare the result with the generated Lean code in the repository. A difference SHALL fail the job, and the message SHALL name each file that differs.

#### Scenario: TC-9-01 Changed Rust function with no new translation fails

- **GIVEN** a change to a proved Rust function and no new translation in the repository
- **WHEN** CI runs
- **THEN** the check of the fresh translation fails

#### Scenario: Translation that agrees passes

- **GIVEN** generated Lean code that a translation of the current Rust code gives
- **WHEN** the check of the fresh translation runs
- **THEN** the check passes

### Requirement: No unfinished proof

The `verify` job SHALL fail when the Lean code of the project contains `sorry` or `admit`, or when a theorem in a "Proof obligations" table depends on `sorryAx`. Lean code in a fixture folder does not count.

#### Scenario: TC-9-02 Theorem with sorry fails

- **GIVEN** a theorem that contains `sorry`
- **WHEN** CI runs
- **THEN** the verification job fails

### Requirement: Only standard axioms

For each theorem in the "Proof obligations" table of an accepted tech design, the `verify` job SHALL list the axioms that the theorem depends on. The permitted axioms are `propext`, `Classical.choice` and `Quot.sound`. A different axiom SHALL fail the job, and the message SHALL name the theorem and the axiom.

#### Scenario: TC-9-03 Theorem that depends on an axiom of Aeneas fails

- **GIVEN** a theorem in a "Proof obligations" table that depends on an axiom that Aeneas generated
- **WHEN** CI runs
- **THEN** the check of the axioms fails and names the theorem and the axiom

### Requirement: Each named theorem exists

The `verify` job SHALL read the "Proof obligations" table of each tech design with the status `accepted`. A table is enforced when a Rust function that its "Rust function it is about" column names exists in the core crate. For each theorem name in an enforced table, the job SHALL find a theorem with that name in the namespace `Strata`. A name with no theorem SHALL fail the job, and the message SHALL name the theorem and the tech design.

#### Scenario: TC-9-04 Named theorem that does not exist fails

- **GIVEN** a theorem that the "Proof obligations" table of an accepted design names, and that does not exist in the Lean code
- **WHEN** CI runs
- **THEN** the check fails and names the theorem

#### Scenario: Table with no function in the core crate is not enforced

- **GIVEN** an accepted design whose table names the Rust function `revision_step`, and no function `revision_step` in the core crate
- **WHEN** the check runs
- **THEN** the check passes for the theorems of that table

#### Scenario: Table becomes enforced when its function exists

- **GIVEN** an accepted design whose table names `job_step`, a function `job_step` in the core crate, and no theorem `Strata.inactive_parks`
- **WHEN** the check runs
- **THEN** the check fails and names `inactive_parks`

#### Scenario: Table of a design that is not accepted does not count

- **GIVEN** a design with a status that is not `accepted`, whose table names a theorem that does not exist
- **WHEN** the check runs
- **THEN** the check passes for that name

### Requirement: Each proved function has a refinement theorem

For each Rust function that the "Rust function it is about" column of an enforced table names, the `verify` job SHALL find a theorem `Strata.<function>_refines`. The theorem states that the Rust function never panics and that its result is the result of the Lean model. A function with no refinement theorem SHALL fail the job, and the message SHALL name the function.

#### Scenario: Function with no refinement theorem fails

- **GIVEN** an accepted table that names the Rust function `job_step`, and no theorem `Strata.job_step_refines`
- **WHEN** the check runs
- **THEN** the check fails and names `job_step`

### Requirement: Crates build on stable Rust

The crates of the workspace SHALL build on stable Rust. No crate SHALL have a `rust-toolchain` file. Only the translation uses the pinned nightly of Charon.

#### Scenario: TC-9-05 Crate with a nightly feature fails on stable

- **GIVEN** a Rust crate that uses a nightly feature
- **WHEN** CI builds it on stable
- **THEN** the build fails

#### Scenario: Toolchain file in the workspace fails

- **GIVEN** a `rust-toolchain` file in a crate of the workspace
- **WHEN** the check of the toolchain runs
- **THEN** the check fails and names the file

### Requirement: Pinned package of Aeneas

The install script SHALL compare the SHA-256 of the package of Aeneas and Charon with the pinned value before it extracts or runs the package. A package that does not agree SHALL stop the job. The script SHALL try three sources in this sequence. The first is the pinned release of Aeneas. The second is the copy in a release of the Strata repository. The third is a build from the pinned commit.

#### Scenario: TC-9-06 Package with a different SHA-256 stops the job

- **GIVEN** a package of Aeneas with a SHA-256 that is not the pinned one
- **WHEN** CI installs it
- **THEN** the job stops before it uses the package

#### Scenario: Second source when the first is not available

- **GIVEN** the release of Aeneas is not available, and the copy in the Strata release agrees with the pinned SHA-256
- **WHEN** the install script runs
- **THEN** it installs the copy from the Strata release

### Requirement: When the verification job runs

The `verify` job SHALL run on each pull request and on each push to `main` that changes a watched file. The watched files are the core crate, the Lean project, the proof scripts and the tech designs. A failure of the job SHALL make the required CI result fail.

#### Scenario: Change to the core crate starts the job

- **GIVEN** a pull request that changes a file in `crates/core/`
- **WHEN** CI runs
- **THEN** the `verify` job runs, and its result is a part of the required CI result
