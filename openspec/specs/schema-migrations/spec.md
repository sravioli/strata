# schema-migrations Specification

## Purpose
Applies the schema of Strata from SQL files that the server binary contains, with the owner role only (tech designs 0003 and 0049).

## Requirements

### Requirement: The binary applies its migrations

`strata migrate` SHALL apply each migration of the binary that the database does not have, in order, as `strata_owner`, directly and not through the pooler. A second run SHALL apply nothing and succeed. A migration that fails SHALL stop the run with an error.

#### Scenario: First run on an empty database

- **GIVEN** an empty database with the four roles of 0003
- **WHEN** an operator runs `strata migrate`
- **THEN** the tables of the migrations exist, and `strata_owner` owns each one

#### Scenario: Second run

- **GIVEN** a database with all the migrations
- **WHEN** an operator runs `strata migrate` again
- **THEN** the command succeeds and changes nothing

### Requirement: A migration does not change after it is applied

The run SHALL fail when the text of a migration that the database has is different from the text in the binary.

#### Scenario: Changed migration

- **GIVEN** a database with migration 1
- **WHEN** a binary with a different text of migration 1 runs `strata migrate`
- **THEN** the command exits with an error and applies nothing
