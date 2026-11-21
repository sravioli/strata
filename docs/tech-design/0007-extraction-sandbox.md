- Issue: [#7](https://github.com/sravioli/strata/issues/7)
- Status: accepted
- Start date: 2026-11-18
- Decided by: sravioli, 2026-11-19

# Extraction sandbox: hostile files go to a process that can reach nothing

## Problem description

A worker must get the text and the structure out of each uploaded file: PDF, DOCX, PPTX, XLSX and CSV. Each file comes from a client, so each file is hostile input. A parser that reads a hostile file can crash, loop, use all memory, or let an attacker run code. This design puts each parse in a separate process that has no credentials, no network and hard limits. Then an attack on a parser reaches only one file.

Goals:

- An exploited parser cannot read or change data of a tenant, and cannot reach the network.
- A file that is too large, too slow or malformed fails one revision, and does not stop the worker.
- The parsers are free and open-source software.

Non-goals:

- Chunking, embedding and indexing of the text (M2, [#70](https://github.com/sravioli/strata/issues/70), [#71](https://github.com/sravioli/strata/issues/71), [#64](https://github.com/sravioli/strata/issues/64)).
- OCR of scanned PDFs. The owner decided that OCR is not in v1, but a later version must add it ([#72](https://github.com/sravioli/strata/issues/72)).
- The corpus of hostile files for tests. The map puts it in M1 ([#48](https://github.com/sravioli/strata/issues/48)).

## Background and references

The worker of ADR 0004 claims a `ProcessDocument` job, opens a tenant transaction and writes the result. The tenancy design (ADR 0001) says that a compromised worker can open a tenant transaction for each tenant. If the worker parses the file in its own process, an exploit of the parser gets the database role of the worker. That is the risk that issue [#7](https://github.com/sravioli/strata/issues/7) names.

ADR 0005 already decides that the extraction process has no storage credentials, and that the worker gives it the bytes. The maximum file size is 100 MB.

Good PDF parsers are native code, and native parsers of complex formats have a long history of memory-safety faults. A parser in safe Rust can still loop or use all memory on a hostile file. Office formats (DOCX, PPTX, XLSX) are ZIP archives of XML files, so a small file can expand to a very large one (a "zip bomb").

Linux gives these mechanisms to confine a process:

- Namespaces: no network, and no view of other processes and files.
- Resource limits (`setrlimit`) and control groups: memory and CPU.
- seccomp: a filter of system calls.
- Landlock: rules for file access.

Tools that combine them exist. nsjail (Google, Apache 2.0) and bubblewrap (used by Flatpak, LGPL) are established tools. Young Rust crates, for example hakoniwa and sandlock, combine the same mechanisms.

References:

- nsjail: https://github.com/google/nsjail
- bubblewrap: https://github.com/containers/bubblewrap
- Landlock: https://docs.kernel.org/userspace-api/landlock.html
- seccomp: https://docs.kernel.org/userspace-api/seccomp_filter.html
- PDFium: https://pdfium.googlesource.com/pdfium/
- Poppler: https://poppler.freedesktop.org/
- The crate `poppler` (GPL): https://lib.rs/crates/poppler
- A third-party comparison of `pdftotext` and PDFium: https://invoice2data.readthedocs.io/latest/backend-benchmark.html
- `pdfium-render`: https://github.com/ajrcarey/pdfium-render
- `calamine`: https://github.com/tafia/calamine

## Requirements

| ID | Requirement |
|---|---|
| E1 | The extraction process has no database credentials, no storage credentials, no other secrets and no network. |
| E2 | One extraction process reads one file, then stops. A process never reads files of two revisions. |
| E3 | Hard limits on memory, CPU time, wall-clock time, output size and the size of expanded archives. A process that reaches a limit is stopped. |
| E4 | A file that fails extraction for a reason in the file sets its revision to `Failed`. The job does not retry it. |
| E5 | A failure outside the file, for example a worker that stops, is a normal job failure, and the job retries (ADR 0004). |
| E6 | The worker treats the output of the extraction process as untrusted input, and checks it before it writes it. |
| E7 | The parsers and the sandbox tool are free and open-source software. |
| E8 | v1 extracts PDF, DOCX, PPTX, XLSX and CSV. |

Quantities:

- 10,000 uploads each hour at the ceiling (ADR 0003), approximately 3 each second.
- Files up to 100 MB, 5 MB on average (ADR 0005).
- 95 % of documents become searchable in 5 minutes or less (ADR 0003). Extraction is one part of that time.

## Proposed design

### One process for each file

```text
worker (trusted)                          extraction process (sandbox)
  | claim job, read bytes from storage      |
  | start sandbox: no network, no secrets,  |
  |   limits, empty file system             |
  |---- bytes on standard input ----------->|
  |                                         | detect format, parse
  |<--- text and structure on standard -----|
  |     output, in a fixed format           |
  | wait: exit code, or stop at the limit   |
  | check the output                        |
  | write the result in a tenant transaction|
```

1. The worker claims a `ProcessDocument` job and reads the file from the storage with its own credentials.
2. The worker starts a new extraction process in the sandbox. It sends the bytes on standard input, and the declared content type as an argument.
3. The extraction process detects the format from the bytes, parses the file and writes the text and the structure to standard output. Then it stops.
4. The worker reads at most the output limit. It stops the process when a limit is reached.
5. The worker checks the output (E6) and writes it in a tenant transaction.

The extraction process is a separate binary of Strata, `strata-extract`. It has no code to reach a database, a storage or the network.

### The sandbox

This design proposes nsjail as the sandbox tool. It applies each mechanism with one configuration:

| Mechanism | Setting |
|---|---|
| Network namespace | A new, empty network namespace. The process has no network interface. |
| Mount namespace | A read-only file system with only the `strata-extract` binary, the program `pdftotext`, and their libraries. No `/proc` of the host, no home directories, no secrets. |
| User namespace | The process runs as a user with no rights. |
| PID namespace | The process sees no other process. |
| Environment | Empty. No variable of the worker passes to the process. |
| Resource limits | Address space, CPU time, open files. No new files (`RLIMIT_FSIZE` is 0). |
| Control group | Memory, CPU and the count of processes: two at most, `strata-extract` and `pdftotext`. |
| seccomp | A list of the system calls that a parser needs, and the calls that start `pdftotext`. Each other call stops the process. |
| Time limit | nsjail stops the process at the wall-clock limit. |

The worker also enforces the wall-clock limit and the output limit itself. Thus a fault in the sandbox configuration does not remove these limits.

Proposed limits:

| Limit | Value |
|---|---|
| Memory | 1 GB |
| CPU time | 60 seconds |
| Wall-clock time | 120 seconds |
| Output | 50 MB of text and structure |
| Expanded size of an archive | 500 MB in total |
| Extraction processes at the same time, for each worker | A bound in the configuration (ADR 0003) |

### Parsers

| Format | Parser | Notes |
|---|---|---|
| PDF | Two parsers: the program `pdftotext` of Poppler (`accurate`), and PDFium through the crate `pdfium-render` (`fast`). The tenant selects one for each upload. | See "Two parsers for PDF". |
| DOCX, PPTX | The crates `zip` and `quick-xml`, with a small reader in `strata-extract` | Only the text and the headings, slides and tables. |
| XLSX | The crate `calamine` | One block of text for each sheet. |
| CSV | The crate `csv` | One block of text for each file. |

For DOCX and PPTX, the reader of archives counts the expanded bytes and stops at the limit, before the control group must stop the process. `calamine` reads an XLSX archive itself. Thus, before `calamine` opens an XLSX file, `strata-extract` adds the expanded sizes in the directory of the archive, and rejects the file above the limit. An archive whose directory gives false sizes reaches the memory limit, and gets `ExtractionMemory`. Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)): before, the limit did not reach XLSX.

Legacy formats (DOC, PPT, XLS) are not in v1 ([#76](https://github.com/sravioli/strata/issues/76)). A revision in a format that is not supported becomes `Failed` with the class `UnsupportedFormat`.

A file that needs a password becomes `Failed` with the class `EncryptedFile`. Two kinds of file need a password: a PDF with a user password, and an Office file with a password. An Office file with a password is a Compound File Binary container with the stream `EncryptedPackage`. `strata-extract` finds that stream, and gives `UnsupportedFormat` to each other container of that type. A PDF with only an owner password opens with no password, and the extraction continues. Strata does not decrypt files in v1 ([#98](https://github.com/sravioli/strata/issues/98)). Amended on 2026-11-21 ([#48](https://github.com/sravioli/strata/issues/48)): before, no class existed for an encrypted file.

### Two parsers for PDF

Both are free and open-source software. They differ in these points:

| Point | PDFium | Poppler |
|---|---|---|
| Licence | BSD-style | GPL. The crate `poppler` says that a program that links it must also be GPL. |
| Use from Rust | The crate `pdfium-render` links the library. | The crates `poppler` and `pdftotext` link the library. Or the sandbox runs the program `pdftotext` of Poppler, and `strata-extract` does not link Poppler. |
| Speed | Faster in the comparison in "Background and references": about 4 ms for each file, against about 21 ms. | Slower. |
| Text quality | Lower in that comparison on invoices with a layout: the right result for 73 of 170 files. | Higher in that comparison: the right result for 146 of 170 files. |
| Origin | The PDF engine of Chromium. Google runs fuzz tests on it. | The PDF library of many Linux desktops. Based on the code of xpdf. |

The comparison is one third-party test on invoices, with templates. It shows a choice between speed and text quality, and 21 ms for each file is also fast. Thus Strata supports both parsers:

- The start of an upload (ADR 0005) has the field `pdf_parser`, with the value `accurate` (Poppler) or `fast` (PDFium). If the tenant does not send the field, the value is `accurate`. Strata ignores the field for formats that are not PDF.
- The revision holds the value. The worker reads it when it starts the extraction, and gives the selected parser to the extraction process.
- A new extraction of an old revision with a different parser is not in v1 ([#77](https://github.com/sravioli/strata/issues/77)). The tenant uploads a new revision.
- Both parsers run in the same sandbox, with the same limits. The sandbox (E1 to E3) does not change with the parser.
- `strata-extract` links PDFium. For Poppler, `strata-extract` starts the program `pdftotext` in the same sandbox, reads its text, and writes the output format. `strata-extract` does not link Poppler.
- Strata is GPL-2.0-or-later ([#50](https://github.com/sravioli/strata/pull/50)), and the `COPYING` file of Poppler is the text of the GPL version 2. Before the first release, the worker image must hold the licence text of Poppler and an offer of its source. Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)): before, the owner had to check the effect of the GPL. The licence of Strata answers that question.

A benchmark in the implementation measures both parsers, and the owner uses the result to check the default:

- Corpus: PDF files of the types that the products of Strata hold, for example reports, slides and invoices, with a known text for each file.
- Measurements: the similarity of the text to the known text, the time for each page, and the peak memory. Also the count of crashes and limits reached.
- Each parser runs inside the sandbox with the same limits.

### Output format

The extraction process writes one JSON document. It holds the format, a list of blocks, and the count of pages or sheets. Each block has its text and its kind, for example a heading, a paragraph, a page, a sheet or a slide. The worker checks it:

- The output is valid JSON with the expected fields, and its size is below the limit.
- Each text is valid UTF-8.
- No field is used as a key, a path or an SQL fragment.

### Results and failures

| Result | Revision | Job |
|---|---|---|
| The process ends with valid output | The worker writes the text. The revision continues to indexing. | `Done` |
| The process crashes, reaches the time limit, or ends with output that fails the check, and the revision has no retry mark | The revision gets the retry mark | A normal failure. The job retries (ADR 0004). |
| The process crashes or reaches the time limit, and the revision has the retry mark. On any run, the process reaches a different limit or rejects the file. | `Failed`, with a class: `ExtractionTimeout`, `ExtractionMemory`, `ExtractionCrash`, `ExtractionOutputTooLarge`, `ArchiveTooLarge`, `UnsupportedFormat`, `EncryptedFile`, `MalformedFile` | `Done`. The job did its work: it decided that the file fails. |
| The process ends with output that fails the check, and the revision has the retry mark | `Failed`, class `InvalidOutput` | `Done` |
| The worker cannot read the file, or the worker stops | No change | A normal failure. The job retries (ADR 0004). |

A result for a reason in the file does not retry, because a second run of the same file gives the same result (E4). A crash, the time limit and output that fails the check are the exceptions. A fault in Strata or in the machine can also cause them, for example a bug in `strata-extract` or a slow machine. Thus the first of these results gives the revision a retry mark. It is a normal job failure, and the job runs again after the backoff. When the revision has the retry mark, one of these results sets the revision to `Failed` with the class of that run: `ExtractionCrash`, `ExtractionTimeout` or `InvalidOutput`. The worker writes the retry mark in the transaction that ends the job, and that transaction checks the `lease_token` (ADR 0004). Thus a worker that stops writes no mark and does not use the retry. A worker whose lease expired cannot write a mark either. The count of attempts of the job does not decide the retry, because a worker that stops also increases it. Amended on 2026-11-21 ([#47](https://github.com/sravioli/strata/issues/47)): before, the count of attempts decided the retry. A file that reaches the time limit at each run thus uses a worker for two time limits. Amended on 2026-11-21 ([#47](https://github.com/sravioli/strata/issues/47)): before, the first crash, time limit or output that failed the check set the revision to `Failed`. The owner decided the retry for `ExtractionCrash` first, and then for `ExtractionTimeout` and `InvalidOutput`. The class goes to the revision. The log of the worker holds the detail, with no content of the file.

A PDF with no text, for example a scan, gives an empty list of blocks. The revision is not `Failed`. It continues with no text, and it holds the class `NoText` so that a later OCR step can find it.

## Alternatives considered

**Parse in the worker process.** This is the simplest and the fastest. But an exploit of a parser then gets the database role of the worker, which can open a tenant transaction for each tenant (ADR 0001). This is the risk that issue [#7](https://github.com/sravioli/strata/issues/7) names.

**A separate container or pod for extraction.** This gives the strongest separation, with a separate kernel namespace set by the platform. But it needs a platform that starts containers, a queue between the worker and the extractor, and more operations. A child process in a sandbox gives the same rules (E1 to E3) on one machine. Issue [#7](https://github.com/sravioli/strata/issues/7) already said to move to a separate pod only if the child process is not sufficient.

**bubblewrap instead of nsjail.** bubblewrap is well tested, because Flatpak uses it on many desktops. But it gives namespaces only. Resource limits, control groups and seccomp need more code around it. nsjail gives all of them in one configuration.

**A Rust sandbox crate, for example hakoniwa or sandlock.** No external binary is then necessary. But these crates are young, and some need a very recent kernel (Landlock network rules need Linux 6.7 or later). A sandbox is a security boundary, so this design prefers an established tool.

**Apache Tika as a separate service.** Tika reads more than a thousand formats, and its licence is Apache 2.0. But it is a Java service with network access, it needs its own sandbox, and it is much larger than five formats need.

**One parser for PDF.** One parser is less code and one less dependency. But PDFium is faster and Poppler gives better text, and different products need different results. Thus the tenant selects for each upload. See "Two parsers for PDF".

## Proof obligations

N/A. The sandbox and the parsers are outside the proved core: they are native code, system calls and process control. Test cases at L1 cover the limits, the isolation and the classes of failure. The corpus of hostile files (M1) tests the parsers.

## Impacts

### Infrastructure

- The worker image holds `strata-extract`, PDFium, the program `pdftotext` of Poppler, and nsjail.
- The sandbox needs user namespaces. In a container, the platform must permit them, or the worker must run with the rights that nsjail needs. M3 decides the platform. See "Unresolved questions".
- Metrics: extractions for each format and each result class.
- Metrics: the time and the peak memory of each extraction, and the count of processes that a limit stopped.
- Alert: an increase of `ExtractionCrash`. A crash can show an attack on a parser.

### Security and tenant isolation

- An exploit of a parser reaches one file: the file that it reads. It cannot reach the database, the storage, the network or the files of a different revision (E1, E2).
- An exploit can give false output for its own file, for example text that is not in the file. It cannot give output for a different file. The worker checks the form of the output (E6).
- An exploit can try to escape the sandbox through a fault in the kernel. seccomp limits the system calls that it can use. This risk stays, and a separate container or virtual machine is the next step if it becomes too high.
- The content type and the file name from the client are not trusted. The extraction process detects the format from the bytes.
- A zip bomb or a file that is slow to parse fails one revision, and the limits protect the worker.

### Data and migration

- A revision holds the class of a failed extraction, the class `NoText` for a file with no text, the retry mark, and the value of `pdf_parser`. The design of [#49](https://github.com/sravioli/strata/issues/49) defines the columns.
- The start of an upload (ADR 0005) gets the optional field `pdf_parser`. A client that does not send it gets `accurate`, so existing clients do not change.
- No other change. The text and its blocks go to tables that the design for indexing (M2) defines.

### Developer

- A new binary, `strata-extract`, in the workspace.
- Local development and L1 tests need Linux with user namespaces, and nsjail in the test image. On a system without them, the tests of the sandbox cannot run.
- The PDFium library is a binary dependency. The build downloads a fixed version of it.

## Test cases

L0: unit tests cover the reader of each format, the check of the output, and the map from each result to a class.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-7-01 | L1 | Given an extraction process. When it tries to open a network connection. Then the attempt fails. | sravioli, 2026-11-19 | pending |
| TC-7-02 | L1 | Given an extraction process. When it reads its environment and tries to read the files of the worker. Then it finds no variable of the worker and no secret. | sravioli, 2026-11-19 | pending |
| TC-7-03 | L1 | Given a DOCX file that expands to more than 500 MB. When the worker extracts it. Then the revision is `Failed` with `ArchiveTooLarge`, the job is `Done`, and the worker continues with other jobs. | sravioli, 2026-11-19 | pending |
| TC-7-04 | L1 | Given a file that makes the parser run without end. When the worker extracts it the first time. Then the process stops at the time limit, the revision gets the retry mark and stays `processing`, and the job retries. When the worker extracts it the second time. Then the revision is `Failed` with `ExtractionTimeout`, and the job is `Done`. Amended on 2026-11-21 ([#47](https://github.com/sravioli/strata/issues/47)). | sravioli, 2026-11-21 | pending |
| TC-7-05 | L1 | Given a file that makes the parser use more than 1 GB. When the worker extracts it. Then the process stops, and the revision is `Failed` with `ExtractionMemory`. | sravioli, 2026-11-19 | pending |
| TC-7-06 | L1 | Given an extraction process that writes more than 50 MB. When the worker reads the output. Then the worker stops the process, and the revision is `Failed` with `ExtractionOutputTooLarge`. | sravioli, 2026-11-19 | pending |
| TC-7-07 | L1 | Given a file with the name `report.pdf` and the bytes of a ZIP archive. When the worker extracts it. Then the process detects the format from the bytes, not from the name. | sravioli, 2026-11-19 | pending |
| TC-7-08 | L1 | Given a scanned PDF with no text. When the worker extracts it. Then the revision is not `Failed`, and it holds the class `NoText`. | sravioli, 2026-11-19 | pending |
| TC-7-09 | L1 | Given one PDF, DOCX, PPTX, XLSX and CSV file with known text. When the worker extracts each one. Then the output holds that text. | sravioli, 2026-11-19 | pending |
| TC-7-10 | L1 | Given a worker that stops while an extraction runs. When the lease expires. Then a different worker claims the job and extracts the file. The first extraction process stops at its time limit at the latest, and nothing writes its output. | sravioli, 2026-11-19 | pending |
| TC-7-11 | L1 | Given two uploads of the same PDF, one with `pdf_parser` set to `fast` and one with no `pdf_parser`. When the worker extracts each one. Then PDFium extracts the first one, Poppler extracts the second one, and each revision holds its value of `pdf_parser`. | sravioli, 2026-11-19 | pending |
| TC-7-12 | L1 | Given a file that makes the parser crash at each run. When the worker extracts it the first time. Then the revision gets the retry mark and stays `processing`, and the job retries. When the worker extracts it the second time. Then the revision is `Failed` with `ExtractionCrash`, and the job is `Done`. | sravioli, 2026-11-21 | pending |
| TC-7-13 | L1 | Given an extraction process that ends with output that fails the check at each run. When the worker extracts the file the first time. Then the revision gets the retry mark and stays `processing`, and the job retries. When the worker extracts it the second time. Then the revision is `Failed` with `InvalidOutput`, and the job is `Done`. | sravioli, 2026-11-21 | pending |
| TC-7-14 | L1 | Given a file that makes the parser crash at each run, and a worker that stops during the first run. When the lease expires and a second worker extracts the file. Then the revision gets the retry mark, and the job retries. When a worker extracts the file again. Then the revision is `Failed` with `ExtractionCrash`. | sravioli, 2026-11-21 | pending |

## Unresolved questions

The owner answered these questions on 2026-11-19:

1. OCR of scanned PDFs is not in v1. A later version must add it ([#72](https://github.com/sravioli/strata/issues/72)). A scanned PDF gets the class `NoText`, so that the OCR step can find it.
2. The sandbox tool is nsjail.
3. Strata supports both PDFium and Poppler. The owner decided this on 2026-11-19, because each has an advantage. See "Two parsers for PDF".
4. The limits in the table stay. They are 1 GB of memory, 60 seconds of CPU and 120 seconds in total. Also 50 MB of output, and 500 MB for an expanded archive.
5. Legacy formats (DOC, PPT, XLS) are not in v1 ([#76](https://github.com/sravioli/strata/issues/76)).

The owner also decided on 2026-11-19 that the tenant selects the parser for each upload, with the field `pdf_parser`.

Implementation resolves these points:

- The benchmark of PDFium and Poppler, to check the default.
- The licence text and the source offer of Poppler in the worker image.
- A check of the licences of PDFium, `pdfium-render`, `calamine`, `zip`, `quick-xml` and `csv`.
- A check that nsjail runs inside the container of the worker on the development system and in CI.
- The list of system calls for seccomp.
- The exact fields of the output format.

These questions belong to later designs:

- OCR, and where it runs (M2). A later version must add it ([#72](https://github.com/sravioli/strata/issues/72)).
- The corpus of hostile files (M1, [#48](https://github.com/sravioli/strata/issues/48)).
- A separate container or virtual machine for extraction, if the risk of a kernel fault becomes too high ([#75](https://github.com/sravioli/strata/issues/75)).
- The platform that permits user namespaces (M3, [#61](https://github.com/sravioli/strata/issues/61)).
