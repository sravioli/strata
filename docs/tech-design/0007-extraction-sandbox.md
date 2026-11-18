- Issue: [#7](https://github.com/sravioli/strata/issues/7)
- Status: draft
- Start date: 2026-11-18
- Decided by: (not decided)

# Extraction sandbox: hostile files go to a process that can reach nothing

## Problem description

A worker must get the text and the structure out of each uploaded file: PDF, DOCX, PPTX, XLSX and CSV. Each file comes from a client, so each file is hostile input. A parser that reads a hostile file can crash, loop, use all memory, or let an attacker run code. This design puts each parse in a separate process that has no credentials, no network and hard limits. Then an attack on a parser reaches only one file.

Goals:

- An exploited parser cannot read or change data of a tenant, and cannot reach the network.
- A file that is too large, too slow or malformed fails one revision, and does not stop the worker.
- The parsers are free and open-source software.

Non-goals:

- Chunking, embedding and indexing of the text (M2).
- OCR of scanned PDFs. See "Unresolved questions".
- The corpus of hostile files for tests. The map puts it in M1.

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
| Mount namespace | A read-only file system with only the `strata-extract` binary and its libraries. No `/proc` of the host, no home directories, no secrets. |
| User namespace | The process runs as a user with no rights. |
| PID namespace | The process sees no other process. |
| Environment | Empty. No variable of the worker passes to the process. |
| Resource limits | Address space, CPU time, open files. No new files (`RLIMIT_FSIZE` is 0). |
| Control group | Memory, CPU and the count of processes. |
| seccomp | A list of the system calls that a parser needs. Each other call stops the process. |
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
| PDF | PDFium, through the crate `pdfium-render` | PDFium is the PDF engine of Chromium, in C++. Its licence is BSD-style. Poppler is GPL, so this design does not use it. |
| DOCX, PPTX | The crates `zip` and `quick-xml`, with a small reader in `strata-extract` | Only the text and the headings, slides and tables. |
| XLSX | The crate `calamine` | One block of text for each sheet. |
| CSV | The crate `csv` | One block of text for each file. |

The reader of archives counts the expanded bytes and stops at the limit, before the control group must stop the process.

Legacy formats (DOC, PPT, XLS) are not in v1. A revision in a format that is not supported becomes `Failed` with the class `UnsupportedFormat`.

### Output format

The extraction process writes one JSON document. It holds the format, a list of blocks, and the count of pages or sheets. Each block has its text and its kind, for example a heading, a paragraph, a page, a sheet or a slide. The worker checks it:

- The output is valid JSON with the expected fields, and its size is below the limit.
- Each text is valid UTF-8.
- No field is used as a key, a path or an SQL fragment.

### Results and failures

| Result | Revision | Job |
|---|---|---|
| The process ends with valid output | The worker writes the text. The revision continues to indexing. | `Done` |
| The process reaches a limit, crashes, or rejects the file | `Failed`, with a class: `ExtractionTimeout`, `ExtractionMemory`, `ExtractionCrash`, `ExtractionOutputTooLarge`, `ArchiveTooLarge`, `UnsupportedFormat`, `MalformedFile` | `Done`. The job did its work: it decided that the file fails. |
| The process ends with output that fails the check | `Failed`, class `InvalidOutput` | `Done` |
| The worker cannot read the file, or the worker stops | No change | A normal failure. The job retries (ADR 0004). |

A result for a reason in the file does not retry, because a second run of the same file gives the same result (E4). The class goes to the revision. The log of the worker holds the detail, with no content of the file.

A PDF with no text, for example a scan, gives an empty list of blocks. The revision is not `Failed`. It continues with no text, and it holds the class `NoText` so that a later OCR step can find it.

## Alternatives considered

**Parse in the worker process.** This is the simplest and the fastest. But an exploit of a parser then gets the database role of the worker, which can open a tenant transaction for each tenant (ADR 0001). This is the risk that issue [#7](https://github.com/sravioli/strata/issues/7) names.

**A separate container or pod for extraction.** This gives the strongest separation, with a separate kernel namespace set by the platform. But it needs a platform that starts containers, a queue between the worker and the extractor, and more operations. A child process in a sandbox gives the same rules (E1 to E3) on one machine. Issue [#7](https://github.com/sravioli/strata/issues/7) already said to move to a separate pod only if the child process is not sufficient.

**bubblewrap instead of nsjail.** bubblewrap is well tested, because Flatpak uses it on many desktops. But it gives namespaces only. Resource limits, control groups and seccomp need more code around it. nsjail gives all of them in one configuration.

**A Rust sandbox crate, for example hakoniwa or sandlock.** No external binary is then necessary. But these crates are young, and some need a very recent kernel (Landlock network rules need Linux 6.7 or later). A sandbox is a security boundary, so this design prefers an established tool.

**Apache Tika as a separate service.** Tika reads more than a thousand formats, and its licence is Apache 2.0. But it is a Java service with network access, it needs its own sandbox, and it is much larger than five formats need.

**Poppler for PDF.** Poppler gives good text. But its licence is GPL. PDFium gives good text with a BSD-style licence.

## Proof obligations

N/A. The sandbox and the parsers are outside the proved core: they are native code, system calls and process control. Test cases at L1 cover the limits, the isolation and the classes of failure. The corpus of hostile files (M1) tests the parsers.

## Impacts

### Infrastructure

- The worker image holds `strata-extract`, PDFium and nsjail.
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

- A revision holds the class of a failed extraction, and the class `NoText` for a file with no text.
- No other change. The text and its blocks go to tables that the design for indexing (M2) defines.

### Developer

- A new binary, `strata-extract`, in the workspace.
- Local development and L1 tests need Linux with user namespaces, and nsjail in the test image. On a system without them, the tests of the sandbox cannot run.
- The PDFium library is a binary dependency. The build downloads a fixed version of it.

## Test cases

L0: unit tests cover the reader of each format, the check of the output, and the map from each result to a class.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-7-01 | L1 | Given an extraction process. When it tries to open a network connection. Then the attempt fails. | (not decided) | pending |
| TC-7-02 | L1 | Given an extraction process. When it reads its environment and tries to read the files of the worker. Then it finds no variable of the worker and no secret. | (not decided) | pending |
| TC-7-03 | L1 | Given a DOCX file that expands to more than 500 MB. When the worker extracts it. Then the revision is `Failed` with `ArchiveTooLarge`, the job is `Done`, and the worker continues with other jobs. | (not decided) | pending |
| TC-7-04 | L1 | Given a file that makes the parser run without end. When the worker extracts it. Then the process stops at the time limit, and the revision is `Failed` with `ExtractionTimeout`. | (not decided) | pending |
| TC-7-05 | L1 | Given a file that makes the parser use more than 1 GB. When the worker extracts it. Then the process stops, and the revision is `Failed` with `ExtractionMemory`. | (not decided) | pending |
| TC-7-06 | L1 | Given an extraction process that writes more than 50 MB. When the worker reads the output. Then the worker stops the process, and the revision is `Failed` with `ExtractionOutputTooLarge`. | (not decided) | pending |
| TC-7-07 | L1 | Given a file with the name `report.pdf` and the bytes of a ZIP archive. When the worker extracts it. Then the process detects the format from the bytes, not from the name. | (not decided) | pending |
| TC-7-08 | L1 | Given a scanned PDF with no text. When the worker extracts it. Then the revision is not `Failed`, and it holds the class `NoText`. | (not decided) | pending |
| TC-7-09 | L1 | Given one PDF, DOCX, PPTX, XLSX and CSV file with known text. When the worker extracts each one. Then the output holds that text. | (not decided) | pending |
| TC-7-10 | L1 | Given a worker that stops while an extraction runs. When the lease expires. Then a different worker claims the job and extracts the file. The first extraction process stops at its time limit at the latest, and nothing writes its output. | (not decided) | pending |

## Unresolved questions

The owner must answer these questions before this design can be accepted:

1. Is OCR of scanned PDFs out of v1? This design says yes. A scanned PDF gets the class `NoText`, and a later design can add OCR.
2. Do you accept nsjail as the sandbox tool?
3. Do you accept PDFium for PDF?
4. Do you accept the limits in the table? They are 1 GB of memory, 60 seconds of CPU, 120 seconds in total, 50 MB of output, and 500 MB for an expanded archive.
5. Do you accept that legacy formats (DOC, PPT, XLS) are not in v1?

Implementation resolves these points:

- A check of the licences of PDFium, `pdfium-render`, `calamine`, `zip`, `quick-xml` and `csv`.
- A check that nsjail runs inside the container of the worker on the development system and in CI.
- The list of system calls for seccomp.
- The exact fields of the output format.

These questions belong to later designs:

- OCR, and where it runs (M2).
- The corpus of hostile files (M1).
- A separate container or virtual machine for extraction, if the risk of a kernel fault becomes too high.
- The platform that permits user namespaces (M3).
