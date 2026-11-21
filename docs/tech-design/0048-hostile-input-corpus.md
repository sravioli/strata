- Issue: [#48](https://github.com/sravioli/strata/issues/48)
- Status: draft
- Start date: 2026-11-21
- Decided by:

# Hostile-input corpus: generated files that each limit of the extraction sandbox must stop

## Problem description

Tech design 0007 (extraction sandbox) puts each parse in a sandbox with hard limits, and gives a class to each failure. A design does not show that the limits work. This design adds a corpus of hostile files and a check that runs the real extraction process on each file. For each file, the corpus states the expected result and the limit that must stop it.

Goals:

- At least one entry permits each class of 0007 for a reason in the file. TC-7-04 and TC-7-05 of 0007 already reach the time limit and the memory limit with a known file. Thus the corpus does not need an entry that reaches them on each run.
- A change to the extraction process, its parsers or its sandbox runs the corpus before the merge.
- A crash of a parser on a corpus file stops the merge.

Non-goals:

- Fuzzing of the readers and the parsers ([#93](https://github.com/sravioli/strata/issues/93)).
- Real malformed files from public sets, to test the PDF parsers ([#94](https://github.com/sravioli/strata/issues/94)).
- The benchmark of PDFium and Poppler. It measures text quality and speed, not limits (0007).
- Files above 100 MB. The start of an upload rejects them with `FileTooLarge` (0006), so they never reach extraction.

## Background and references

Tech design 0007 decides these points:

- A separate binary, `strata-extract`, parses one file in nsjail and writes one JSON document to standard output.
- The limits are 1 GB of memory, 60 seconds of CPU time and 120 seconds of wall-clock time. The output limit is 50 MB, and the limit for the expanded content of an archive is 500 MB.
- The reader of DOCX and PPTX counts the expanded bytes. Before `calamine` opens an XLSX file, `strata-extract` adds the expanded sizes in the directory of the archive.
- The classes for a reason in the file are `ExtractionTimeout`, `ExtractionMemory`, `ExtractionCrash`, `ExtractionOutputTooLarge`, `ArchiveTooLarge`, `UnsupportedFormat` and `MalformedFile`. The worker gives `InvalidOutput` to output that fails its check.
- The tenant selects the PDF parser for each upload: `accurate` (the program `pdftotext` of Poppler) or `fast` (PDFium).

`docs/agents/testing.md` defines the levels. An L1 test runs against real local dependencies, and it fails when a dependency is not available. It does not skip. Issue #48 noted that this kind of test was outside L0 to L3. The owner decided on 2026-11-21 that it is an L1 test.

Office files (DOCX, PPTX, XLSX) are ZIP archives of XML parts. Each entry of a ZIP archive has its compressed size and its expanded size twice: in a local header and in the central directory. A program can write false sizes in both. DEFLATE compresses a long run of one byte approximately 1,000 times, so a file of 1 MB can expand to approximately 1 GB. Several entries of the central directory can also point to the same compressed data.

A password-protected Office file is not a ZIP archive. Office puts the encrypted package in a Compound File Binary container, the container of the legacy formats (DOC, PPT, XLS). A PDF file can have two passwords. With only an owner password, a reader can open the file and read the text. With a user password, a reader needs the password.

References:

- Tech design 0007, extraction sandbox ([#7](https://github.com/sravioli/strata/issues/7)).
- `docs/agents/testing.md`, test levels.
- ZIP format: https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT
- Office encryption, [MS-OFFCRYPTO]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-offcrypto/
- PDF security handlers, ISO 32000-2, section 7.6.
- A better zip bomb (entries that share data): https://www.bamsoftware.com/hacks/zipbomb/

## Requirements

| ID | Requirement |
|---|---|
| H1 | Each entry of the corpus has a file, the expected result and the limit or check that must stop it. |
| H2 | The expected result is one class, a set of permitted classes, or "text": the extraction succeeds. `ExtractionCrash` is never an expected result. |
| H3 | Git holds no generated hostile file. A script builds each file again on each run, and the same script gives the same bytes each time. |
| H4 | The check runs the real `strata-extract` in the real sandbox, with the limits of 0007, through the same code that the worker uses to start it. |
| H5 | Each PDF entry runs two times: one time with `accurate` and one time with `fast`. |
| H6 | The check fails, and does not skip, when nsjail, user namespaces or a corpus file is not available. |
| H7 | CI runs the check on each pull request and each push to `main` that changes the extraction process, its dependencies, its sandbox configuration or the corpus. |

Quantities:

- Approximately 30 entries, and approximately 40 runs with H5.
- The largest generated file is the CSV file of TC-48-08: 60 MB. All generated files together are below 100 MB.
- Most runs end in less than 1 second. A run that reaches the wall-clock limit takes 120 seconds.
- Each sandbox can use 1 GB of memory. Thus the check starts at most 4 sandboxes at the same time, and not more than the count of CPU cores. More parses than cores can push a slow file past the wall-clock limit.

## Proposed design

### The parts

```text
corpus manifest (JSON, in git)        generator (scripts/, Python)
  one entry for each file:              reads the manifest,
  name, format, recipe,                 builds each file from its recipe,
  declared type, expected result,       writes it to target/hostile-corpus/
  limit or check that stops it                 |
             |                                 v
             +------------> corpus check (L1 test target of strata-extract)
                              for each entry, and for each PDF parser:
                                start strata-extract in the sandbox
                                (the code of the worker), send the file
                                compare the class with the expected result
```

1. **The manifest.** One JSON file next to the test target of `strata-extract`. The owner decides the categories of entries in this design (TC-48-01 to TC-48-11). An agent can add an entry to a category with no new decision. A new category needs a change to this design.
2. **The generator.** A script in `scripts/`, Python 3 with the standard library only, as for each repo tool (`docs/agents/testing.md`). It reads the manifest and builds each file from its recipe into `target/hostile-corpus/`. It has no random input, so it gives the same bytes on each run. Its tests in `scripts/tests/` check the cheap properties of each recipe, for example the sizes in the directory of an archive.
3. **The check.** An L1 test target of `strata-extract`, with `required-features = ["corpus"]`. For each entry it starts the extraction through the function that the worker calls. Then it compares the result with the expected result. The check runs one extraction for each entry, with no job and no revision. Thus the retry rules of a job do not apply to it. A failure names the entry, the parser and the class that it got.

A crash file that fuzzing finds ([#93](https://github.com/sravioli/strata/issues/93)) can go into git as a regression file. Its manifest entry has the recipe `file` and the path of the file. That is the only exception to H3.

### Categories of entries

0007 decides that the extraction process detects the format from the bytes. This design adds a rule: detection reads the start of the file. Thus a file that is a valid PDF and also a valid ZIP archive is a PDF, because it starts with `%PDF`.

| Category | What the files do | The limit or check that must stop them |
|---|---|---|
| Zip bomb with true sizes | One XML part expands to more than 500 MB. The content is whitespace, not text, so the output limit cannot stop it first. | The count of the reader (DOCX, PPTX). The sum of the directory sizes (XLSX). |
| Entries that share data | Many entries of the central directory point to the same compressed data. | The sum of the directory sizes (XLSX). The count of the reader (DOCX, PPTX). |
| False directory sizes | The headers say that each entry is small. The data expands to more than 500 MB. | The count of the reader (DOCX, PPTX). The memory limit, or the CRC check of the ZIP reader (XLSX). |
| XML entity expansion | An XML part has a DTD with nested entities. | Office parts do not need a DTD. The reader of DOCX and PPTX rejects a DTD. No entity expands in XLSX. |
| Encrypted files | A PDF with a user password, and a PDF with only an owner password. An Office file with a password, and a ZIP entry with the encryption flag. | Format detection, and the open step of the parser. |
| Type that does not agree with the content | The declared type and the bytes do not agree, or the bytes are of no supported format. | Format detection from the bytes. |
| Truncated and malformed files | A DOCX file with no central directory, and a PDF file cut in half. | The parser. |
| Compressed PDF stream | A content stream expands to a very large count of text operators. | The output limit, the memory limit or the time limit. |
| Large output | A CSV file of 60 MB of text. | The output limit. |
| Deep nesting | XML with 1,000,000 nested elements, and a PDF with 1,000,000 nested arrays. | The parser, inside the limits. |

### When it runs

- Locally: `make corpus`. It runs the generator, then the check.
- CI: a new job `corpus` runs `make corpus`. The job `changes` of `ci.yml` gets a filter and an output `extract`. The filter holds the crate of `strata-extract`, `Cargo.lock`, the configuration of the sandbox, the generator and the manifest. The job has a timeout of 15 minutes.
- The job `corpus` goes into the `needs` list of `ci-ok`, the only required check. Thus a failure of the corpus stops the merge.
- The L1 job runs `cargo test --workspace --features l1`, not `--all-features`. Thus it does not run the corpus. Clippy keeps `--all-features`, so it still checks the code of the corpus.

## Alternatives considered

**A separate kind of test, next to L0 to L3.** A new kind gets its own rules and its own command. But the check is an L1 test: it runs against real local dependencies, nsjail and the kernel. A new kind adds rules that L1 already has. The owner selected L1 on 2026-11-21.

**Run the corpus on each pull request, or only on a schedule.** Each pull request is the safest choice, but most pull requests do not touch extraction, and a run can take minutes. A run only on a schedule finds a fault after the merge. The path filter runs the corpus exactly when a change can break it.

**Hostile files in git.** Real files are more realistic, but antivirus software can flag them, and each file has a licence question. Generated files test the same limits. Real files are the subject of [#94](https://github.com/sravioli/strata/issues/94).

**Real malformed files from a public set.** The Stressful PDF Corpus of the PDF Association has approximately 32,500 files from the bug trackers of PDF tools. These files test the native PDF parsers, which generated files rarely do. But this design found no licence statement for the set, and CI must then download the files. The owner moved this to [#94](https://github.com/sravioli/strata/issues/94).

**Fuzzing in this design.** Fuzzing finds new crashes, and a static corpus does not. But `cargo-fuzz` needs a nightly toolchain and a long run time. The owner moved it to [#93](https://github.com/sravioli/strata/issues/93).

**A generator in Rust, inside the test.** The crate `zip` can write archives. But most recipes write bytes that a correct writer refuses to write, for example false sizes. The repo tools are in Python (`docs/agents/testing.md`), and the owner selected a script.

## Proof obligations

N/A. The corpus tests native parsers, system calls and process limits, which are outside the proved core (tech design 0009). The test cases below are L1.

## Impacts

### Infrastructure

- A new CI job, `corpus`, and a new path filter, `extract`. Production does not change.
- The CI runner must permit user namespaces for nsjail. 0007 already lists this check for implementation. Ubuntu can limit user namespaces with AppArmor, so the job can need a change to that setting.

### Security and tenant isolation

- The generated files are hostile to other programs too, for example a file manager that makes previews. They stay in `target/`, which git ignores.
- A regression file from fuzzing goes into git. Its manifest entry says that it is hostile.
- The corpus has no tenant data. It does not change the tenant boundary.

### Data and migration

N/A. The corpus adds no schema and no stored data.

### Developer

- `make corpus` needs Linux with user namespaces, nsjail and Python 3. On a system without them, the check fails (H6).
- The run takes minutes, because some files run to the wall-clock limit. Most changes do not need it locally.
- The L1 command in `docs/agents/testing.md` changes from `--all-features` to `--features l1`, and the document gets the command of the corpus.

## Test cases

L0: the tests of the generator in `scripts/tests/` check the properties of each recipe.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-48-01 | L1 | Given the manifest. When the check runs. Then each entry has a file, an expected result and the limit or check that must stop it. At least one entry permits each class of 0007 for a reason in the file, except `ExtractionCrash`. No entry expects `ExtractionCrash`. | | pending |
| TC-48-02 | L1 | Given a DOCX, a PPTX and an XLSX file with true sizes. Each has an XML part that expands to more than 500 MB of whitespace. When the check extracts each one. Then the class is `ArchiveTooLarge`. | | pending |
| TC-48-03 | L1 | Given an XLSX file with 1,000 sheet entries. They point to the same compressed data of 1 MB. When the check extracts it. Then the class is `ArchiveTooLarge`. `calamine` does not open the file. | | pending |
| TC-48-04 | L1 | Given a DOCX, a PPTX and an XLSX file. Their headers say that each entry is below 1 KB, but their data expands to more than 500 MB. When the check extracts each one. Then the DOCX and the PPTX get `ArchiveTooLarge`. The XLSX gets `ExtractionMemory` or `MalformedFile`. | | pending |
| TC-48-05 | L1 | Given a DOCX, a PPTX and an XLSX file. Each has an XML part with a DTD of nested entities. When the check extracts each one. Then no entity expands. The DOCX and the PPTX get `MalformedFile`. The XLSX gets `MalformedFile`, or its text with no expanded entity. | | pending |
| TC-48-06 | L1 | Given four files: a PDF with only an owner password, and a PDF with a user password. Also a DOCX file with a password, and a DOCX file with a ZIP entry that has the encryption flag. When the check extracts each one. Then the first PDF gives its text. The second PDF gets the class of open question 1. The DOCX with a password gets `UnsupportedFormat` (option B) or `EncryptedFile` (option A). The last DOCX gets `MalformedFile`. | | pending |
| TC-48-07 | L1 | Given three files. The first has the declared type `application/pdf` and the bytes of a DOCX file. The second has bytes in no supported format. The third is a valid PDF that also has a ZIP central directory at the end. When the check extracts each one. Then the first gives the text of the DOCX. The second gets `UnsupportedFormat`. The third gives the text of the PDF. | | pending |
| TC-48-08 | L1 | Given a CSV file of 60 MB of text. When the check extracts it. Then the class is `ExtractionOutputTooLarge`. | | pending |
| TC-48-09 | L1 | Given a PDF with a compressed content stream that expands to more than 1 GB of text operators. When the check extracts it with each parser. Then the class is `ExtractionOutputTooLarge`, `ExtractionMemory` or `ExtractionTimeout`. | | pending |
| TC-48-10 | L1 | Given four files: a DOCX file with no central directory, and a PDF cut in half. Also a DOCX file with 1,000,000 nested elements, and a PDF with 1,000,000 nested arrays. When the check extracts each one. Then each one gives text or gets `MalformedFile`. | | pending |
| TC-48-11 | L1 | Given any entry. When its result is `ExtractionCrash`, or a class that its entry does not permit. Then the check fails, and the failure names the entry, the parser and the class. | | pending |

## Unresolved questions

The owner answered these questions on 2026-11-21:

1. The corpus is an L1 test, with a manifest of entries. The owner decides the categories in this design.
2. CI runs it on each pull request and each push to `main` that changes extraction, with a path filter.
3. A script generates the files. Real malformed files are a later decision ([#94](https://github.com/sravioli/strata/issues/94)).
4. Fuzzing is a later decision ([#93](https://github.com/sravioli/strata/issues/93)).

The owner must answer these questions:

1. **A PDF with a user password.** 0007 has no class for an encrypted file. Option A: a new class `EncryptedFile`, so that a client can tell its user to remove the password. Option A also changes the results table of 0007 and the list of `error_class` in the readiness design ([#47](https://github.com/sravioli/strata/issues/47)). Option B: `MalformedFile`, with no change to other designs. This design recommends option A. With option A, a password-protected Office file can also get `EncryptedFile`, because `strata-extract` can find the encrypted package in the container.
2. Approval of the test cases TC-48-01 to TC-48-11.

Implementation resolves these points:

- The exact recipe of each entry, and the format of the manifest.
- A check that nsjail runs on the CI runner (0007 already lists it).
- The real run time of the corpus in CI, below the timeout of 15 minutes.
- The form of the L1 command when only some crates of the workspace have the feature `l1`, for example `--features strata-extract/l1`.
- Which class the XLSX of TC-48-04 gets. The result depends on the ZIP reader of `calamine`, so the entry permits two classes.
