Issue: #49

# Proposal

## Why

Tech design 0049 builds the walking skeleton in four changes. This is change 3. After change 2, a finish creates a `ProcessDocument` job, but no process runs it. This change adds `strata worker`. The worker claims the job and extracts the text of the PDF in the sandbox of 0007. It writes the text parts in the transaction that ends the job. Change 4 needs these text parts for the search.

## What Changes

- Add the subcommand `strata worker`. It connects as `strata_worker` through PgBouncer. It uses the values of 0049: a lease of 300 seconds, a renewal each 100 seconds, and 2 jobs at the same time. The poll is 1 second, and doubles after each empty poll up to 10 seconds.
- Add `ClaimTx` (0004, 0049). The claim selects one job with `for update skip locked`. It gives `LeaseExpired` to `job_step` for a `Running` job with an expired lease. It sets the tenant context of the job, reads the tenant status, and gives `Claim(tenant_active)` to `job_step`. Only `ClaimTx::into_tenant` with an `allow` decision gives the tenant transaction of the job.
- Add the renewal of the lease and the checks of `lease_token`. A write with a `lease_token` that changed writes nothing.
- Add the end of a job with failure: the backoff of `job.rs`, and `Parked` at the maximum of attempts (0005).
- Add the call of the cleanup of `Done` jobs from the loop of the worker, each 60 seconds (`cleanup.rs` of change 2).
- Add the read of an object by the worker, with the credentials of the worker (0006). The worker reads at most 100 MB.
- Add the crate `strata-extract`. It reads the bytes on standard input and detects the format from the bytes. For a PDF, it runs `pdftotext` and writes one JSON document on standard output (0049). Each other format gets `UnsupportedFormat`.
- Add the nsjail configuration of 0007 for `strata-extract` and `pdftotext`, with the limits of 0007 and a seccomp list that blocks `io_uring_setup`, `io_uring_enter` and `io_uring_register` (TC-49-10).
- Add the check of the output in the worker (0007, E6) and the split of a block above 64 KB (0049). Map each result to an event of `revision_step`.
- Add a third migration with `text_parts`, its policy, the extension `btree_gin`, the GIN index on `(tenant_id, tsv)` and the grants of `strata_worker` on `text_parts`.
- Add the transaction that ends a job (0049). It has these steps:
  - the lock of the document and `revision_step`,
  - the text parts and `ready_revision`,
  - the `Superseded` revisions and the removal of their text parts,
  - the write of the revision, and the end of the job with the check of `lease_token`.
- Add `strata admin job requeue --tenant <name>` (0005, 0049).
- Add a script that builds nsjail 3.6 from source at the pinned commit of 0049. `make setup` and the `l1` job of CI use it. If the CI runner blocks the user namespaces of nsjail, CI loads an AppArmor profile for the nsjail binary only.
- Run each extraction in a child cgroup of a delegated cgroup, so that the control group limits of 0007 work with no root. `make l1` and the `l1` job of CI run the tests in a delegated cgroup.

Not in this change:

- The search route, the status routes of 0047, `make dev` and TC-49-01. Change 4 adds them. Thus `strata_api` gets no grant on `text_parts` in this change. Change 4 adds `select` on `text_parts` with the search.
- The formats DOCX, PPTX, XLSX and CSV ([#102](https://github.com/sravioli/strata/issues/102)), and PDFium with the field `pdf_parser` ([#103](https://github.com/sravioli/strata/issues/103)).
- The hostile-input corpus ([#48](https://github.com/sravioli/strata/issues/48)). It runs on `strata-extract` after this change.
- The cleanup of revisions in `AwaitingUpload` and of the objects of `Superseded` revisions ([#69](https://github.com/sravioli/strata/issues/69)).
- The `RemoveFiles` job. No route creates it before [#69](https://github.com/sravioli/strata/issues/69). The worker claims only `ProcessDocument` jobs.

## Decisions for this change

The owner decided these points on 2026-11-22. This change amends 0049 with them:

- **`text_parts` moves to change 3.** The transaction that ends a job writes the text parts. Without the table, change 3 sets `Ready` with no text.
- **Requeue is in change 3.** It belongs to the job lifecycle of 0005.
- **TC-49-02 and TC-49-06.** This change checks their clauses on text parts with a `select` on `text_parts`. Their clauses on the search come with the search route in change 4.
- **nsjail.** A script builds nsjail 3.6, the latest tag, from source at the pinned commit `f78475530b46d0186111a9096b30725f816b55fe`. nsjail has no release binaries.
- **CI and AppArmor.** If the runner blocks the user namespaces of nsjail, CI loads an AppArmor profile that permits them for the nsjail binary only. CI does not set `kernel.apparmor_restrict_unprivileged_userns` to 0.
- **A delegated cgroup.** Task 1.2 showed that the control group v2 limits of 0007 work with no root only in a delegated cgroup that holds no process ([#49](https://github.com/sravioli/strata/issues/49#issuecomment-7082691219)). The setting `STRATA_CGROUP` names that cgroup. The worker makes one child cgroup in it for each extraction, and removes the child after the run. The worker reads `oom_kill` in `memory.events` of the child before it reads the exit code, because a process can exit with 0 after the kernel stops its child for memory.
  - Locally, `make l1` runs the tests in `systemd-run --user --scope -p Delegate=yes`.
  - In CI, the step that runs `make l1` creates `/sys/fs/cgroup/strata` with `sudo`, gives it to the user `runner`, and moves its shell into a leaf child. The move is in the same step, because each step starts a new shell.
  - The image of the worker needs a delegated cgroup ([#43](https://github.com/sravioli/strata/issues/43)).
- **The match is leakproof.** Under the policy of 0003, PostgreSQL 19 never used the GIN index for `tsv @@ query`, because the function `ts_match_vq` is not `leakproof`. A search then reads each text part of the tenant. Thus `dev/roles.sql` marks `ts_match_vq(tsvector, tsquery)` as `leakproof`. Only a superuser can set the mark, in each database, so the setup of each deploy must run it ([#43](https://github.com/sravioli/strata/issues/43)).
- **A failure of nsjail.** nsjail exits with 255 when it cannot start the sandbox. The cause is not in the file. Thus it is a normal failure of the job: the backoff, no change to the revision and no retry mark. After 5 attempts, the job is `Parked` with `JobFailed`.

The agent decided these points. The owner reviews them in the pull request:

- **An object above 100 MB at the read of the worker.** The signed PUT URL stays valid for 15 minutes, so a client can replace the object after the finish. The worker stops the read after 100 MB. This is the row "The worker cannot read the file" of 0007: no change to the revision, and a normal failure of the job. After 5 attempts, the job is `Parked` with `JobFailed`. An object that does not exist gets the same result. `revision_step` does not change.
- **A test program for the limits.** The L1 tests of the limits run a test program of `strata-tests` in place of `strata-extract`, with the same nsjail configuration. One argument selects one behaviour of the program:
  - open a network connection, or read the environment,
  - use more than 1 GB, or run with no end, or crash,
  - write more than 50 MB, or write output that fails the check,
  - call `io_uring_setup`, `io_uring_enter` or `io_uring_register`.

  `strata-extract` has no code for tests. The tests with real files (TC-7-07, TC-7-08, the PDF of TC-7-09) run the real `strata-extract` and `pdftotext`.
- **The limits in the tests.** The worker takes its limits as one value. The values of 0007 are the default. The L1 tests of the time limit use shorter limits, so a test does not wait 120 seconds. Each test of a limit checks the class that the limit gives.
- **`pdftotext` from the system.** The worker and the tests use the `pdftotext` of the package of the system (`poppler-utils`). `make setup` warns when it is missing. A pinned version comes with the image of the worker (M3, [#43](https://github.com/sravioli/strata/issues/43)).
- **Three paths in the settings.** `STRATA_NSJAIL` names the nsjail binary, `STRATA_EXTRACT` names the `strata-extract` binary, and `STRATA_CGROUP` names the delegated cgroup. With a path that is missing, or a cgroup that the worker cannot write, `strata worker` stops with an error that names the variable.
- **One thread in `strata-extract`.** `pids.max` counts threads, so with the limit of 2, `strata-extract` cannot start a thread while `pdftotext` runs. `pdftotext -` reads all of standard input before it writes output. Thus `strata-extract` writes the bytes, closes the pipe, and then reads the output. Its exit codes are below 128 and are not 255, so that they are different from a signal and from a failure of nsjail. The process is PID 1, so `strata-extract` turns a child that stops on a signal into its crash exit code.
- **Two column names.** `sqlfluff` rejects the keywords `location` and `text` as column names (RF04). Thus the columns of `text_parts` are `location_number` and `part_text`, not the `location` and `text` of 0049.
- **The storage settings of the worker** are the six variables of change 2. The worker reads them in the same way as the API.

## Test cases

This change carries the test cases that it can make true, each as a scenario with its TC ID:

- 0003: TC-3-17.
- 0004: TC-4-07, TC-4-10.
- 0005: TC-5-02, TC-5-03, TC-5-04, TC-5-05, TC-5-07, TC-5-09.
- 0007: TC-7-01, TC-7-02, TC-7-04, TC-7-05, TC-7-06, TC-7-07, TC-7-08, TC-7-10, TC-7-12, TC-7-13, TC-7-14.
- 0047: TC-47-16.
- 0049: TC-49-03, TC-49-10.

These test cases get a scenario with the marker `PENDING`, because a part of them needs a later change:

| Test case | The part in this change | The part that waits | Change or issue |
|---|---|---|---|
| TC-49-02 | A scenario with no TC ID checks the text parts with a `select`. | The search returns revision 2 only. | 4 |
| TC-49-06 | A scenario with no TC ID checks the four parts with a `select`. | The search for a word in the last part. | 4 |
| TC-7-09 | A scenario with no TC ID checks the PDF. | DOCX, PPTX, XLSX and CSV. | [#102](https://github.com/sravioli/strata/issues/102) |

These test cases stay `pending` in their designs:

| Test cases | Reason | Change or issue |
|---|---|---|
| TC-7-03 | It needs DOCX. | [#102](https://github.com/sravioli/strata/issues/102) |
| TC-7-11 | It needs PDFium and `pdf_parser`. | [#103](https://github.com/sravioli/strata/issues/103) |
| TC-48-01 to TC-48-11 | The corpus runs after this change. | [#48](https://github.com/sravioli/strata/issues/48) |
| TC-3-10, TC-3-12, TC-3-19, TC-4-01, TC-4-03, TC-4-09, TC-49-01, TC-49-05, TC-49-07, TC-49-08, the TCs of 0047 except TC-47-09 and TC-47-16 | They need the status routes or the search. | 4 |

## Capabilities

### New Capabilities

- `extraction-sandbox`: `strata-extract`, the nsjail configuration, the limits, the check of the output, and the map from each result to a class of 0007.
- `document-text`: the table `text_parts`, its index, the split of a block above 64 KB, and the text parts of a `Ready` revision.

### Modified Capabilities

- `job-queue`: the claim, the lease, the end of a job with success or failure, `Parked`, the read of the object, and the loop of `strata worker`.
- `revision-lifecycle`: the transaction that ends a job writes the result of `revision_step`, and the worker does no extraction when `may_extract` is false (TC-49-03, TC-47-16).
- `operator-command`: `strata admin job requeue` (TC-5-07).
- `tenant-isolation`: the grants of `strata_worker` on `text_parts`. A job of one tenant reads no row of a different tenant (TC-3-17, TC-4-07). A change of the tenant status applies to the next claim (TC-4-10).
- `dev-services`: `make setup` builds nsjail, and the `l1` job of CI runs the sandbox tests.

## Impact

- `crates/strata/migrations/`: a new migration `V3`.
- `crates/strata/src/`: the worker, `ClaimTx`, the read of an object, the check of the output, the transaction that ends a job, and `strata admin job requeue`.
- `crates/strata-extract/`: a new crate, with no dependency on a database, a storage or a network.
- `crates/strata-tests/`: L1 tests against PostgreSQL, Garage and nsjail, and the test program for the limits.
- `crates/strata/`: the nsjail configuration and its seccomp list.
- `scripts/install-nsjail.sh` and `.github/requirements/tools.env`: the build and the pinned commit of nsjail. `Makefile` and `.github/workflows/ci.yml`: the build of nsjail and, if necessary, the AppArmor profile.
- `core` does not change, so the Lean translation and the proofs do not change.
- New dependencies of `strata-extract`: only the crates that the JSON output needs. `cargo deny` checks them.
- `docs/tech-design/0049-walking-skeleton.md`: amended notes (committed on this branch).
- No data exists, so no migration of data is necessary.
