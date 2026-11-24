# Spec Delta

## ADDED Requirements

### Requirement: One transaction ends a job

The worker SHALL end a `ProcessDocument` job with one tenant transaction. It SHALL lock the document with `for update`, read the revision, and write the result of `revision_step` for the result of the extraction. For a new `Ready` state, it SHALL also write the text parts, `ready_revision`, and `Superseded` with `status_at` for each older revision in `AwaitingUpload`, `Uploaded` or `Ready`.

#### Scenario: The worker writes the result of the function

- **GIVEN** an `Uploaded` revision and a result of its extraction
- **WHEN** the job ends
- **THEN** the state, `retry_mark`, `error_class` and `no_text` that the database holds are the result of `revision_step` for the event of that result

#### Scenario: A Ready revision becomes the ready revision of its document

- **GIVEN** an `Uploaded` revision 2 of a document, with revision 1 in `Uploaded`
- **WHEN** the job of revision 2 ends with text
- **THEN** revision 2 is `Ready`, `ready_revision` of the document is 2, and revision 1 is `Superseded` with a new `status_at`

#### Scenario: Two jobs of one document

- **GIVEN** the jobs of revisions 1 and 2 of one document, each with text
- **WHEN** both jobs end at the same time, in each order
- **THEN** exactly one revision of the document is `Ready`, and it is revision 2
- **AND** revision 1 is `Superseded`

#### Scenario: TC-49-03 The job of an older revision ends late

- **GIVEN** revision 2 of a document is `Ready`
- **WHEN** the job of revision 1 ends after that
- **THEN** revision 1 stays `Superseded`, the worker writes no text for it, and the job is `Done`

### Requirement: The end of a job follows the result

A result that sets the retry mark SHALL end the job with `Fail`. Each other result SHALL end the job with `Succeed`, also a result that sets `Failed`. A write of the revision and the end of the job SHALL be in the same transaction, and a `lease_token` that changed SHALL roll back both.

#### Scenario: A lease_token that changed

- **GIVEN** a job whose `lease_token` changed after the claim
- **WHEN** the worker ends the job with a result
- **THEN** the transaction rolls back, and the revision and its text parts do not change

### Requirement: No extraction when the function says no

At the claim, the worker SHALL read the revision. When `may_extract` is false, the job SHALL end with `Succeed`, and the worker SHALL start no extraction.

#### Scenario: TC-47-16 A superseded revision does no extraction

- **GIVEN** revision 1 is `processing` or `uploading`, and revision 2 of the same document becomes ready
- **WHEN** the job of revision 1 runs, or the client finishes revision 1
- **THEN** the job does no extraction, no new job exists, and revision 1 stays `superseded`
