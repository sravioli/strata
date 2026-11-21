---
status: "accepted"
date: 2026-11-19
decision-makers: sravioli
tech-design: docs/tech-design/0007-extraction-sandbox.md
---

# Extraction of each file in a separate process in an nsjail sandbox, with two parsers for PDF

## Context and Problem Statement

A worker must get the text and the structure out of each uploaded file. Each file comes from a client, so each file is hostile input. If the worker parses the file in its own process, an exploit of a parser gets the database role of the worker. That role can open a tenant transaction for each tenant (ADR 0001). How does Strata extract files so that an attack on a parser reaches only one file? The issue is [#7](https://github.com/sravioli/strata/issues/7).

## Considered Options

* A separate process for each file, in an nsjail sandbox
* Parse in the worker process
* A separate container or pod for extraction
* bubblewrap instead of nsjail
* A Rust sandbox crate, for example hakoniwa or sandlock
* Apache Tika as a separate service
* One parser for PDF

## Decision Outcome

Chosen option: "A separate process for each file, in an nsjail sandbox". It gives no credentials, no network and hard limits on one machine, with one established tool.

The decision has these parts:

* The worker reads the file from storage. It starts the binary `strata-extract` in nsjail, sends the bytes on standard input, and reads JSON on standard output.
* One process reads one file, then stops.
* The process has no credentials, no network, no environment variables of the worker, and no writable files.
* Limits: 1 GB of memory, 60 seconds of CPU and 120 seconds in total. Also 50 MB of output, and 500 MB for an expanded archive. The worker also enforces the time and output limits itself.
* The worker treats the output as untrusted input, and checks it before it writes it.
* Formats in v1: PDF, DOCX, PPTX, XLSX and CSV. The process detects the format from the bytes. Legacy formats (DOC, PPT, XLS) get the class `UnsupportedFormat`.
* A file that needs a password gets the class `EncryptedFile`: a PDF with a user password, or an Office file with a password. Strata does not decrypt files in v1 ([#98](https://github.com/sravioli/strata/issues/98)) (Amended on 2026-11-21 ([#48](https://github.com/sravioli/strata/issues/48))).
* PDF has two parsers. The tenant selects one for each upload with the optional field `pdf_parser`: `accurate` (the program `pdftotext` of Poppler) or `fast` (PDFium). The default is `accurate`. The revision holds the value.
* A failure for a reason in the file sets the revision to `Failed` with a class, and the job is `Done` with no retry. A failure outside the file retries (ADR 0004). A crash, the time limit or output that fails the check retries one time, because a fault in Strata can also cause it. The second such result sets the revision to `Failed` with `ExtractionCrash`, `ExtractionTimeout` or `InvalidOutput` (Amended on 2026-11-21 ([#47](https://github.com/sravioli/strata/issues/47))). A retry mark on the revision counts the retry, not the count of attempts of the job. Thus a worker that stops does not use the retry (Amended on 2026-11-21 ([#47](https://github.com/sravioli/strata/issues/47))).
* A PDF with no text continues with the class `NoText`. OCR is not in v1, but a later version must add it.

### Consequences

* Good, because an exploit of a parser reaches only the file that it reads.
* Good, because a hostile file fails one revision and does not stop the worker.
* Good, because a tenant can select speed or text quality for each PDF.
* Bad, because the sandbox needs user namespaces, also inside containers. Implementation must check this on the development system and in CI.
* Bad, because a fault in the kernel can let an exploit escape the sandbox. seccomp limits the system calls. A separate container or virtual machine is the next step if this risk becomes too high.
* Bad, because two PDF parsers are more code and more dependencies.
* Bad, because Poppler is GPL. Strata is GPL-2.0-or-later, so the worker image must hold the licence text of Poppler and an offer of its source (Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85))).
* Not decided: the benchmark of the two parsers, which checks the default. Also the list of system calls for seccomp, the exact fields of the output, and OCR (M2).
