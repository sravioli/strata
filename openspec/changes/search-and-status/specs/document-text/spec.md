# Spec Delta

## MODIFIED Requirements

### Requirement: A block above 64 KB goes in parts

The worker SHALL split a block above 64 KB at the last whitespace before 64 KB. When the block has no whitespace there, it SHALL split at 64 KB, at the start of a UTF-8 character. The parts of one block SHALL have the location of the block.

#### Scenario: A page of 200 KB goes in four parts

- **GIVEN** a page of 200 KB of text
- **WHEN** the worker writes it
- **THEN** it is in four parts of at most 64 KB, each with the location of the page
- **AND** the text of the four parts, in the order of `part`, is the text of the page

#### Scenario: TC-49-06 A page of 200 KB

- **GIVEN** a page of 200 KB of text
- **WHEN** the worker writes it
- **THEN** it is in four parts of at most 64 KB, and a search for a word in the last part returns the page
- **PENDING** Task 5.1 adds the search clause to the test.

#### Scenario: A block with no whitespace

- **GIVEN** a block of 100 KB with no whitespace and characters of more than one byte
- **WHEN** the worker splits it
- **THEN** each part has at most 64 KB and is valid UTF-8

### Requirement: Text parts exist only for a Ready revision

The worker SHALL write the text parts of a revision in the transaction that sets it `Ready`, with one statement. The transaction that sets a revision `Superseded` SHALL remove its text parts, with one statement.

#### Scenario: A newer Ready revision removes the text parts of the older one

- **GIVEN** revision 1 of a document is `Ready` with its text parts
- **WHEN** revision 2 becomes `Ready`
- **THEN** revision 1 is `Superseded`, and no text part of revision 1 exists
- **AND** the text parts of revision 2 exist

#### Scenario: TC-49-02 Revision 2 becomes Ready

- **GIVEN** revision 1 of a document is `Ready`
- **WHEN** revision 2 becomes `Ready`
- **THEN** revision 1 is `Superseded`, its text parts do not exist, and a search returns revision 2 only
- **PENDING** Task 5.1 adds the search clause to the test.

#### Scenario: A Failed revision has no text parts

- **GIVEN** a revision that becomes `Failed`
- **WHEN** the job ends
- **THEN** no text part of the revision exists
