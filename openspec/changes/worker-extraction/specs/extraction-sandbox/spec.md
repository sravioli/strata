# Spec Delta

## Purpose

Runs the parse of each hostile file in a separate process with no credentials, no network and hard limits. Thus an attack on a parser reaches only one file. Each result gets a class of tech design 0007.

## ADDED Requirements

### Requirement: The extraction process detects the format from the bytes

`strata-extract` SHALL read the bytes of the file on standard input, and detect the format from the bytes only. It SHALL NOT use the file name or the declared content type to select a parser. A PDF SHALL go to `pdftotext`. Each other format SHALL give `UnsupportedFormat`.

#### Scenario: TC-7-07 A ZIP archive with the name report.pdf

- **GIVEN** a file with the name `report.pdf` and the bytes of a ZIP archive
- **WHEN** the worker extracts it
- **THEN** the process detects the format from the bytes, not from the name
- **PENDING** Task 6.1 adds the test.

#### Scenario: A format that is not PDF

- **GIVEN** the bytes of a ZIP archive
- **WHEN** the worker extracts it
- **THEN** the revision is `Failed` with `UnsupportedFormat`, and the job is `Done`

#### Scenario: A PDF with a content type that is not PDF

- **GIVEN** the bytes of a PDF with the declared content type `text/plain`
- **WHEN** the worker extracts it
- **THEN** `pdftotext` extracts its text

### Requirement: The extraction process writes one JSON document

For a PDF, `strata-extract` SHALL write on standard output one JSON document with the fields `format`, `pages` and `blocks`. Each block SHALL have `kind` `page`, its `index` from 1, and its `text`. A PDF with no text SHALL give an empty list of blocks. `strata-extract` SHALL use one thread. Its exit codes SHALL be below 128 and SHALL NOT be 255. When `pdftotext` stops on a signal, `strata-extract` SHALL exit with its crash code.

#### Scenario: The known text of a PDF

- **GIVEN** a PDF of two pages with a known text on each page
- **WHEN** the worker extracts it
- **THEN** the output has `format` `pdf`, `pages` 2 and one block of kind `page` for each page, with its text

#### Scenario: TC-7-09 The known text of each format

- **GIVEN** one PDF, DOCX, PPTX, XLSX and CSV file with known text
- **WHEN** the worker extracts each one
- **THEN** the output holds that text
- **PENDING** The skeleton extracts only PDF. The scenario "The known text of a PDF" checks the PDF. DOCX, PPTX, XLSX and CSV come with #102.

#### Scenario: TC-7-08 A scanned PDF with no text

- **GIVEN** a scanned PDF with no text
- **WHEN** the worker extracts it
- **THEN** the revision is not `Failed`, and it holds the class `NoText`
- **PENDING** Task 6.1 adds the test.

### Requirement: A PDF that the parser cannot read gets a class

A PDF that needs a user password SHALL give `EncryptedFile`. A PDF with only an owner password SHALL open with no password, and the extraction SHALL continue. A PDF that `pdftotext` cannot parse for a reason in the file SHALL give `MalformedFile`. These classes SHALL NOT retry.

#### Scenario: A PDF with a user password

- **GIVEN** a PDF with a user password
- **WHEN** the worker extracts it
- **THEN** the revision is `Failed` with `EncryptedFile`, and the job is `Done`

#### Scenario: A PDF with only an owner password

- **GIVEN** a PDF with a known text and only an owner password
- **WHEN** the worker extracts it
- **THEN** the revision is `Ready` with that text

#### Scenario: A PDF with a broken structure

- **GIVEN** a file that starts with `%PDF-` and has no valid structure
- **WHEN** the worker extracts it
- **THEN** the revision is `Failed` with `MalformedFile`, and the job is `Done`

### Requirement: The sandbox has no network, no environment and no files of the worker

The worker SHALL run each extraction process in nsjail. The process SHALL have a new network namespace with no interface, a new PID namespace and a user namespace with no rights. The file system SHALL be read-only and hold only `strata-extract`, `pdftotext` and their libraries. The environment SHALL be empty. The process SHALL NOT create files.

#### Scenario: TC-7-01 No network

- **GIVEN** an extraction process
- **WHEN** it tries to open a network connection
- **THEN** the attempt fails

#### Scenario: TC-7-02 No environment and no secrets

- **GIVEN** an extraction process
- **WHEN** it reads its environment and tries to read the files of the worker
- **THEN** it finds no variable of the worker and no secret

### Requirement: The sandbox blocks io_uring

The seccomp list of the sandbox SHALL block `io_uring_setup`, `io_uring_enter` and `io_uring_register`. seccomp does not see the operations of a ring, so a ring can bypass the list.

#### Scenario: TC-49-10 Each io_uring call fails

- **GIVEN** the nsjail configuration of `strata-extract`
- **WHEN** the extraction process calls `io_uring_setup`, `io_uring_enter` or `io_uring_register`
- **THEN** each call fails, and the process does no I/O through a ring

### Requirement: The limits of an extraction

Each extraction SHALL have the limits of 0007. They are 1 GB of memory, 60 seconds of CPU time, 120 seconds of wall-clock time, 50 MB of output, and two processes. The worker SHALL also stop the process at the wall-clock limit and at the output limit itself.

#### Scenario: TC-7-05 More than 1 GB of memory

- **GIVEN** a file that makes the parser use more than 1 GB
- **WHEN** the worker extracts it
- **THEN** the process stops, and the revision is `Failed` with `ExtractionMemory`

#### Scenario: TC-7-06 More than 50 MB of output

- **GIVEN** an extraction process that writes more than 50 MB
- **WHEN** the worker reads the output
- **THEN** the worker stops the process, and the revision is `Failed` with `ExtractionOutputTooLarge`

#### Scenario: TC-7-04 The time limit retries one time

- **GIVEN** a file that makes the parser run without end
- **WHEN** the worker extracts it the first time
- **THEN** the process stops at the time limit, the revision gets the retry mark and stays `processing`, and the job retries
- **WHEN** the worker extracts it the second time
- **THEN** the revision is `Failed` with `ExtractionTimeout`, and the job is `Done`

### Requirement: Each extraction has its own control group

Each extraction SHALL run in its own child cgroup of the delegated cgroup of `STRATA_CGROUP`. After the run, the worker SHALL read `oom_kill` in `memory.events` of that child before it reads the exit code, and SHALL remove the child. An `oom_kill` above 0 SHALL be the result `ExtractionMemory`, also when the exit code is 0.

#### Scenario: The kernel stops the child of the process for memory

- **GIVEN** an extraction process whose child uses more than 1 GB, and which exits with 0 after the kernel stops the child
- **WHEN** the worker reads the result
- **THEN** the result is `ExtractionMemory`, and the child cgroup of the run does not exist

### Requirement: A failure of the sandbox is a normal failure

When nsjail exits with 255, the worker SHALL end the job with `Fail`. The revision SHALL NOT change and SHALL NOT get the retry mark.

#### Scenario: nsjail cannot start the sandbox

- **GIVEN** an `Uploaded` revision with no retry mark, and a run where nsjail exits with 255
- **WHEN** the job ends
- **THEN** the revision does not change, and the job is `Waiting` with its backoff

### Requirement: The worker checks the output

The worker SHALL treat the output as untrusted input. The output SHALL be valid JSON with the fields of the format. Each text SHALL be valid UTF-8. The size SHALL be below the limit. No field SHALL become a key, a path or an SQL fragment. Output that fails the check SHALL be the result `InvalidOutput`.

#### Scenario: TC-7-13 Output that fails the check retries one time

- **GIVEN** an extraction process that ends with output that fails the check at each run
- **WHEN** the worker extracts the file the first time
- **THEN** the revision gets the retry mark and stays `processing`, and the job retries
- **WHEN** the worker extracts it the second time
- **THEN** the revision is `Failed` with `InvalidOutput`, and the job is `Done`

#### Scenario: Each form of output that fails the check

- **WHEN** the worker checks output in one of these forms: not JSON, a missing field, a text that is not UTF-8, or a block with an `index` of 0
- **THEN** the result is `InvalidOutput`

### Requirement: A crash retries one time

A process that crashes SHALL be the result `ExtractionCrash`. With no retry mark, the revision SHALL get the mark and the job SHALL retry. With the mark, the revision SHALL be `Failed` with `ExtractionCrash`. Only the transaction that ends the job SHALL write the mark.

#### Scenario: TC-7-12 A crash at each run

- **GIVEN** a file that makes the parser crash at each run
- **WHEN** the worker extracts it the first time
- **THEN** the revision gets the retry mark and stays `processing`, and the job retries
- **WHEN** the worker extracts it the second time
- **THEN** the revision is `Failed` with `ExtractionCrash`, and the job is `Done`

#### Scenario: TC-7-14 A worker that stops does not use the retry

- **GIVEN** a file that makes the parser crash at each run, and a worker that stops during the first run
- **WHEN** the lease expires and a second worker extracts the file
- **THEN** the revision gets the retry mark, and the job retries
- **WHEN** a worker extracts the file again
- **THEN** the revision is `Failed` with `ExtractionCrash`

### Requirement: A worker that loses its lease writes no output

When the lease of a job expires, the extraction process of the first worker SHALL stop at its time limit at the latest. Nothing SHALL write its output.

#### Scenario: TC-7-10 A worker that stops while an extraction runs

- **GIVEN** a worker that stops while an extraction runs
- **WHEN** the lease expires
- **THEN** a different worker claims the job and extracts the file
- **AND** the first extraction process stops at its time limit at the latest, and nothing writes its output
