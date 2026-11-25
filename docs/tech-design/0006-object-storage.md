- Issue: [#6](https://github.com/sravioli/strata/issues/6)
- Status: accepted
- Start date: 2026-11-18
- Decided by: sravioli, 2026-11-18

# Object storage: the S3 API, with a free and open-source server first

## Problem description

Strata keeps the original file of each revision. A client sends the file, a worker reads it to extract text, and a client can download it again. This design decides where the files go, how they get there, and how Strata keeps them apart for each tenant.

Goals:

- One interface in the code, with more than one storage backend possible.
- A first backend that is free and open-source software, and that Strata can host itself.
- File bytes do not pass through the API.
- The tenant boundary holds for each file.

Non-goals:

- Hosting, replication, backup and cost. Milestone M3 decides them ([#61](https://github.com/sravioli/strata/issues/61)).
- The extraction of text from a file. Issue [#7](https://github.com/sravioli/strata/issues/7) decides it.

## Background and references

Issue [#6](https://github.com/sravioli/strata/issues/6) first recommended Azure Blob Storage as the first backend. On 2026-11-18 the owner required a storage that is free and open-source. Earlier, the owner said that Strata can support more than one storage backend (map [#1](https://github.com/sravioli/strata/issues/1)).

The Rust crate `object_store` (Apache Arrow project, version 0.14.2) gives one API for many backends. It supports Amazon S3 and S3-compatible servers, Azure Blob Storage, Google Cloud Storage, local disk and memory. Its `Signer` trait creates signed URLs. These types implement `Signer`: `AmazonS3`, `GoogleCloudStorage` and `MicrosoftAzure`. `LocalFileSystem` and `InMemory` do not implement it. For Azure, a credential from Microsoft Entra ID gives a user delegation SAS, so no account key is necessary.

Free and open-source servers that use the S3 API:

| Server | Licence | State on 2026-11-18 |
|---|---|---|
| MinIO, community edition | AGPLv3 | No binary releases since October 2025. The repository is archived. |
| Garage | AGPLv3 | Maintained. Written in Rust. Presigned URLs are implemented. Made for small, self-hosted clusters. |
| SeaweedFS | Apache 2.0 (not yet checked) | Maintained. Presigned URLs are supported. More parts to operate: master, volume and filer servers. |
| Ceph RADOS Gateway | LGPL | Maintained. A large system to operate. |

The capacity design (ADR 0003) sets 10,000 uploads each hour at the ceiling and a test corpus of 5,000,000 documents. The job queue design (ADR 0004) requires that a job exists exactly when the transaction that causes it commits.

References:

- `object_store`, trait `Signer`: https://docs.rs/object_store/latest/object_store/signer/trait.Signer.html
- `object_store`, Azure signer source: https://github.com/apache/arrow-rs-object-store/blob/main/src/azure/credential.rs
- Garage, S3 compatibility: https://garagehq.deuxfleurs.fr/documentation/reference-manual/s3-compatibility/
- SeaweedFS, Amazon S3 API: https://github.com/seaweedfs/seaweedfs/wiki/Amazon-S3-API
- MinIO repository: https://github.com/minio/minio
- Microsoft Learn, "Create a user delegation SAS": https://learn.microsoft.com/en-us/rest/api/storageservices/create-user-delegation-sas

## Requirements

| ID | Requirement |
|---|---|
| O1 | The first deployed backend is free and open-source software that Strata can host itself. |
| O2 | The code uses one storage interface. A change of backend is a change of configuration. |
| O3 | File bytes go from the client to the storage, and from the storage to the client, without the API. |
| O4 | A client never gets a storage credential. It gets only a signed URL for one file, one method and a short time. |
| O5 | The API gives a signed URL only after `authorize` gives "allow" (ADR 0002). |
| O6 | A file is part of the corpus only after the API checks it. The job for the file exists exactly when the file is part of the corpus. |
| O7 | A file has a maximum size. A file above the maximum never becomes part of the corpus. |
| O8 | After `DeleteDocument`, the files of the document are removed from the storage. |
| O9 | A file that a client did not finish uploading is removed after a fixed time. |
| O10 | L1 tests run against the same kind of server as production, in a container. |

Quantities:

- 10,000 uploads each hour at the ceiling, which is approximately 3 each second (ADR 0003).
- 5,000,000 documents in the test corpus (ADR 0003).
- The average size of a file is not measured. This design assumes 5 MB (owner, 2026-11-18). Then the corpus is approximately 25 TB, before replication.
- The maximum file size is 100 MB (owner, 2026-11-18).

The assumption of 5 MB comes from estimates of usual sizes, not from a measurement:

| Kind of file | Usual size (estimate) |
|---|---|
| PDF | 0.2 MB to 5 MB. Text reports are at the low end, scans and files with many images at the high end. |
| DOCX | 50 KB to 8 MB. Embedded images make a file large. |
| PPTX | 1 MB to 20 MB, mostly images |
| XLSX | 20 KB to 5 MB |
| CSV | 10 KB to more than 50 MB |

A sample of real files from a product, for example "alpha", replaces this assumption when it is available.

## Proposed design

### One interface: `object_store` and the S3 API

Strata uses the traits `ObjectStore` and `Signer` of the crate `object_store`. It does not write its own storage interface. The configuration selects the backend.

Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): the crate `rusty-s3` 0.12 replaces `object_store` (owner, 2026-11-22). With the feature `aws`, `object_store` adds `aws-lc-rs`, a second crypto backend. With each set of features, `make deny` fails, because `object_store` depends on `chrono` and `humantime`, and each has an advisory that it is not maintained (RUSTSEC-2026-0542, RUSTSEC-2026-0543). `rusty-s3` signs S3 requests only. The API signs a PUT URL for a client. It also signs its own requests that read the size of an object and remove an object. The HTTP client of `strata` sends them. Strata has a small storage module of its own, for the S3 API only. Each S3-compatible server is still a change of configuration. Azure Blob Storage now needs its own client.

The first backend is a free and open-source server that uses the S3 API, through the `AmazonS3` type of the crate. The first server is Garage. The owner accepted it, with its licence AGPLv3, on 2026-11-18. Each server and cloud service with an S3 API can replace it with a change of configuration. Azure Blob Storage stays possible through the `MicrosoftAzure` type, with a user delegation SAS.

A backend must implement `Signer`. Thus local disk and memory are not backends for the API. Development and tests use the same server as production, in a container.

### Upload

```text
client               API                       storage          database
  |  1. start upload  |                           |                 |
  |------------------>| authorize UploadDocument  |                 |
  |                   | revision AwaitingUpload --------------------->|
  |<------------------| signed PUT URL (15 min)   |                 |
  |  2. PUT bytes ------------------------------->|                 |
  |  3. finish upload |                           |                 |
  |------------------>| HEAD: size and existence  |                 |
  |                   |-------------------------->|                 |
  |                   | revision Uploaded + job  -------------------->|
  |<------------------| 202 Accepted              |                 |
```

1. **Start.** The client sends the file name, the content type and the size. It can also send the `document_id` of a document that exists. Then the revision is the next revision of that document. With no `document_id`, the API creates a new document, and the revision is revision 1. The API calls `authorize` for `UploadDocument`. A `document_id` of a different tenant gives the same answer as a document that does not exist. The API checks the bound on `Waiting` jobs (ADR 0004) and the bound on revisions in `AwaitingUpload` (see "Removal"). It rejects a declared size above the maximum with `413`, and creates no revision (Amended on 2026-11-21 ([#49](https://github.com/sravioli/strata/issues/49))). It creates the revision in the state `AwaitingUpload` and returns a signed PUT URL for the key of that revision. The URL expires after 15 minutes. The time limits the start of the PUT, not its length. The storage checks the expiry when the request arrives, and a transfer that starts in time continues to its end. Thus a slow transfer of 100 MB does not fail.
2. **Send.** The client sends the bytes to the storage with the URL. The URL permits a PUT of one key only.
3. **Finish.** The client tells the API that the upload is complete. The API reads the size of the object from the storage. If the object does not exist, the API answers `409 Conflict` and changes nothing (Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49))). If the size is above the maximum, the API removes the object, sets the revision to `Failed` with the class `FileTooLarge`, and rejects the upload. Otherwise one transaction sets the revision to `Uploaded` and creates its `ProcessDocument` job (O6). A "finish" of a revision that is `superseded` creates no job, and gives the same answer (Amended on 2026-11-21 ([#47](https://github.com/sravioli/strata/issues/47))).

A second "finish" for the same revision changes nothing and gives the same answer. Thus a client can try again after a network failure.

The `Signer` of `object_store` signs no headers, so its signed URL cannot limit the size of an upload. Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): the PUT URL of `rusty-s3` signs only the header `host`, so the same reason applies. The S3 API can sign a `content-length` header, but reports say that Garage rejects a signed `content-length`. Thus the check of step 3 is the limit (O7). A client can put a large object in its own key. That object never becomes part of the corpus, and step 3 or the cleanup removes it.

Amended on 2026-11-26 ([#154](https://github.com/sravioli/strata/issues/154)): the URL permits more PUTs until it expires, also after the finish. Thus the finish keeps the `ETag` of the object that it checked, in `revisions.etag`. The worker reads the object with `If-Match` and that `ETag`. If the object changed after the finish, the revision becomes `Failed` with `MalformedFile`. Issue [#153](https://github.com/sravioli/strata/issues/153) decides a class of its own. If the object is above the maximum, the revision becomes `Failed` with `FileTooLarge`. In both cases the worker removes the object after the end of the job. Thus a second PUT never reaches the corpus, and its object does not stay.

Amended on 2026-11-26 ([#154](https://github.com/sravioli/strata/issues/154)): the HEAD of step 3 runs between two transactions. The finish reads the revision and ends its transaction. Then it reads the size and the `ETag`. Then it does the steps of the identity design again ([#21](https://github.com/sravioli/strata/issues/21)), locks the revision, and writes only if the revision is still `AwaitingUpload`. Thus a slow storage holds no connection of the pool and no row lock.

### Keys

The key of an original file is `tenants/{tenant_id}/documents/{document_id}/revisions/{revision}/original`. The key is a label for operators. It is not an access control: only signed URLs and the credentials of the API and the worker give access. Later designs add keys under the same revision, for example extracted text.

### Download

The client asks the API for a file. The API calls `authorize` for `ReadDocument`. If the decision is "allow", the API returns a signed GET URL that expires after 5 minutes. If the document belongs to a different tenant, the answer is the same as for a document that does not exist (ADR 0002).

### Removal

`DeleteDocument` marks the document as removed in the database. The search stops to find it at once. In the same transaction, the API creates a job of a new kind, `RemoveFiles`, which removes each file of the document from the storage. A second run of `RemoveFiles` gives the same result, because the removal of a key that does not exist is not an error (ADR 0004, Q4).

A cleanup task removes revisions in the state `AwaitingUpload` that are older than 2 hours, and their objects (O9). The walking skeleton does not run this cleanup. It waits for the principal of the cleanup task in [#69](https://github.com/sravioli/strata/issues/69) (Amended on 2026-11-21 ([#49](https://github.com/sravioli/strata/issues/49))). For a revision that is `superseded`, it removes only the object and keeps the revision (Amended on 2026-11-21 ([#47](https://github.com/sravioli/strata/issues/47))). A PUT must start in the 15 minutes of its URL, so 2 hours leaves a margin for a slow transfer.

The count of revisions in `AwaitingUpload` has a bound for all tenants (ADR 0003, S3). At the bound, the start of an upload gets 503 with `Retry-After`. Each such revision can hold an object of any size until the cleanup. Thus the bound and the cleanup time limit the storage of unfinished uploads. The bound is not for each tenant, because Strata does not limit the load of one tenant (ADR 0003).

Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)): the cleanup time was 24 hours, and the count had no bound. `FileTooLarge` and the field `document_id` are also new.

`RemoveFiles` and the cleanup task need a principal and an action. A `RemoveFiles` job is a job principal with the action `RemoveDocumentFiles` of ADR 0002 (Amended on 2026-11-21 ([#69](https://github.com/sravioli/strata/issues/69))). Issue [#69](https://github.com/sravioli/strata/issues/69) decides the principal of the cleanup task.

### Credentials

| Process | Storage access |
|---|---|
| API | Create signed URLs. Read the size of an object. Remove an object. |
| Worker | Read and remove objects. |
| Extraction process (#7) | None. The worker gives it the bytes. |
| Client | Only signed URLs for one key. |

The API and the worker get the storage credentials from the secret store of the deployment (M3). No credential is in the code or in the repository.

## Alternatives considered

**Azure Blob Storage first, as issue [#6](https://github.com/sravioli/strata/issues/6) first recommended.** It needs no server to operate, and `object_store` can sign with a user delegation SAS. Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): `rusty-s3` has no Azure signer, so Azure needs its own client. But it is not free and open-source software, and the owner requires that (O1). It stays possible as a second backend.

**MinIO.** For years it was the usual self-hosted server with an S3 API. But its community edition has had no binary releases since October 2025, and its repository is archived. Strata would have to build it and fix its security problems.

**SeaweedFS instead of Garage.** Its licence is probably Apache 2.0, which permits more than AGPLv3, and it is made for very many files. But it has more parts to operate, and its presigned URLs need careful configuration behind a reverse proxy. The owner accepted AGPLv3 for Garage, so SeaweedFS stays the next choice if Garage fails the implementation checks.

**Local disk or a shared file system.** This is the simplest for one server. But `LocalFileSystem` cannot sign URLs, so the bytes must pass through the API. Also, each copy of the API needs the same shared file system (ADR 0003, S1).

**Bytes through the API.** The client sends the file to the API, and the API writes it to the storage. This works with each backend. But each upload then holds memory and a connection in the API for the time of the transfer. The API also needs a large bound on the request body. A signed URL keeps the API small and stateless.

## Proof obligations

N/A. This design connects services and does not add a function with a rule. L1 test cases check the signed URLs, the check of step 3 and the removal. The state machine of a revision, with the new state `AwaitingUpload`, belongs to the design of the revision lifecycle.

## Impacts

### Infrastructure

- A new service: the storage server. M3 decides its hosting, its replication and its backups.
- Metrics: the count of uploads that start and that finish, and the count of rejections for size.
- Metrics: the count of objects that the cleanup removes, and the latency of the storage.
- Alert: the count of `RemoveFiles` jobs that are `Parked`.
- Rollout: this design is part of the first version. There is no earlier state.

### Security and tenant isolation

- One bucket holds the files of each tenant. The boundary is in the API: a signed URL names one key, and the API gives it only after `authorize`.
- A signed URL that leaks gives access to one file until it expires. That is 15 minutes for an upload, and 5 minutes for a download.
- A compromised API or worker can read each file with its credentials. This is the same risk that ADR 0001 states for these processes.
- The bytes of a file are input that is not trusted. The API never reads them. The extraction process reads them in its sandbox (#7).
- The content type and the file name from the client are not trusted. The API keeps them as data and never uses them as a key or a path.
- Encryption at rest: the deployment encrypts the disks of the storage server (M3).

### Data and migration

- A new state of a revision: `AwaitingUpload`.
- A new class of a `Failed` revision: `FileTooLarge`.
- A new kind of job: `RemoveFiles`.
- The table of revisions holds the size and the content type of each file.

### Developer

- Local development and L1 tests run the storage server in a container, next to PostgreSQL.
- The crate `object_store` is a new dependency, with the features `aws` and, later, `azure`. Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): the new dependency is `rusty-s3` 0.12, with the feature `rustcrypto` only.

## Test cases

L0: unit tests cover the key of each file and the rules of step 3.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-6-01 | L1 | Given a signed PUT URL for one key. When a client uses it to write a different key. Then the storage refuses the write. | sravioli, 2026-11-18 | pending |
| TC-6-02 | L1 | Given a signed PUT URL. When a client uses it after it expires. Then the storage refuses the write. | sravioli, 2026-11-18 | pending |
| TC-6-03 | L1 | Given an upload of a file above the maximum size. When the client finishes the upload. Then the API rejects it, the object is removed, the revision is `Failed` with `FileTooLarge`, and no job exists. Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)). | sravioli, 2026-11-18 | pending |
| TC-6-04 | L1 | Given an upload that the client started but did not send. When the client finishes the upload. Then the API answers with an error, and no job exists. | sravioli, 2026-11-18 | pending |
| TC-6-05 | L1 | Given an upload that the client finished. When the client finishes it a second time. Then the API gives the same answer, and one job exists. | sravioli, 2026-11-18 | pending |
| TC-6-06 | L1 | Given a document of tenant A. When an account of tenant B asks for its download URL. Then the API answers as for a document that does not exist. | sravioli, 2026-11-18 | pending |
| TC-6-07 | L1 | Given a document with files. When an account removes the document. Then a search does not find it, and after the `RemoveFiles` job no file of the document is in the storage. | sravioli, 2026-11-18 | pending |
| TC-6-08 | L1 | Given an upload that started 3 hours ago and did not finish. Amended on 2026-11-21 ([#85](https://github.com/sravioli/strata/issues/85)): it was 25 hours. When the cleanup task runs. Then the revision and its object are removed. | sravioli, 2026-11-18 | pending |
| TC-6-09 | L2 | Given a deployed Strata with its storage server. When an account uploads a document through the public API and then downloads it. Then the bytes are the same, and the document becomes searchable. | sravioli, 2026-11-18 | pending |

## Unresolved questions

The owner answered on 2026-11-18:

- Garage is the first server. Its licence, AGPLv3, is accepted. Strata uses Garage as a separate server and does not change it.
- The maximum file size is 100 MB.
- A signed URL for an upload expires after 15 minutes, and one for a download after 5 minutes. The time limits the start of a transfer, not its length.
- The assumed average size of a file is 5 MB, until a sample of real files replaces it.

Implementation resolves these points:

- A check that the Garage S3 API accepts the signed URLs of `rusty-s3` (Amended on 2026-11-22 ([#49](https://github.com/sravioli/strata/issues/49)): it was `object_store`), before the first L1 test.
- A check that Garage, like Amazon S3, checks the expiry of a signed URL only when the request arrives.

These questions belong to later designs:

- The state machine of a revision with `AwaitingUpload` ([#49](https://github.com/sravioli/strata/issues/49)).
- Hosting, replication, backups and encryption of the disks (M3, [#61](https://github.com/sravioli/strata/issues/61)).
- Azure Blob Storage or a different service as a second backend ([#74](https://github.com/sravioli/strata/issues/74)).
