---
status: "accepted"
date: 2026-11-18
decision-makers: sravioli
tech-design: docs/tech-design/0006-object-storage.md
---

# Object storage through the S3 API, with Garage as the first server, and signed URLs for clients

## Context and Problem Statement

Strata keeps the original file of each revision. The storage must be free and open-source software, and more than one backend must be possible. File bytes must not pass through the API, and the tenant boundary must hold for each file. Which storage and which interface does Strata use? The issue is [#6](https://github.com/sravioli/strata/issues/6).

## Considered Options

* The S3 API through `object_store`, with Garage as the first server
* Azure Blob Storage first
* MinIO
* SeaweedFS instead of Garage
* Local disk or a shared file system
* Bytes through the API

## Decision Outcome

Chosen option: "The S3 API through `object_store`, with Garage as the first server", because Garage is free and open-source. Each S3-compatible server can then replace it with a change of configuration.

The decision has these parts:

* The code uses the traits `ObjectStore` and `Signer` of the crate `object_store`. A backend must implement `Signer`.
* Garage is the first server. Its licence, AGPLv3, is accepted. Strata uses it as a separate server and does not change it.
* Local development and L1 tests run Garage in a container.
* A client gets only signed URLs for one key and one method. An upload URL expires after 15 minutes, and a download URL after 5 minutes. The time limits the start of a transfer, not its length.
* An upload has three steps: start, send, finish. The start can name a document that exists, for a new revision. At "finish", the API checks the size. A file above the maximum makes the revision `Failed` with `FileTooLarge`. Otherwise one transaction sets the revision to `Uploaded` and creates its job (Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85))).
* The maximum file size is 100 MB.
* The key of a file is a label. Access comes only from signed URLs and from the credentials of the API and the worker.
* `DeleteDocument` creates a `RemoveFiles` job. A cleanup task removes uploads that did not finish in 2 hours. It keeps a `superseded` revision and removes only its object. A "finish" of a `superseded` revision creates no job (Amended on 2026-11-21 ([#47](https://github.com/sravioli/strata/issues/47))). The count of such uploads has a bound. Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)): it was 24 hours, with no bound. A `RemoveFiles` job uses the job action `RemoveDocumentFiles` of ADR 0002 (Amended on 2026-11-21 ([#69](https://github.com/sravioli/strata/issues/69))). Issue [#69](https://github.com/sravioli/strata/issues/69) decides the principal of the cleanup task.
* The assumed average size of a file is 5 MB. The test corpus is then approximately 25 TB before replication.

### Consequences

* Good, because the storage is free and open-source, and Strata can host it.
* Good, because a change to a different S3-compatible server, or to Azure Blob Storage, is a change of configuration.
* Good, because the API holds no file bytes, so it stays small and stateless.
* Good, because local tests use the same server as production.
* Bad, because Strata must operate, replicate and back up a storage server (M3).
* Bad, because the signed URL of `object_store` cannot limit the size of an upload (Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85))). The check at "finish" and the cleanup task must catch large files.
* Bad, because a signed URL that leaks gives access to one file until it expires.
* Not decided: hosting, replication, backups and encryption of the disks (M3). Implementation must check that Garage accepts the signed URLs of `object_store`.
