- Issue: [#47](https://github.com/sravioli/strata/issues/47)
- Status: draft
- Start date: 2026-11-20
- Decided by: (filled in on the decision)

# Document readiness: a status resource that clients poll, with webhooks later

## Summary

A client learns that a revision is ready when it reads the status of that revision. v1 has no push. The API gives five public statuses: `uploading`, `processing`, `ready`, `failed` and `superseded`. They are separate from the internal states. The response tells the client how long to wait before the next read, and the API rejects a client that reads too frequently. A client can read up to 100 revisions in one request. Webhooks come later ([#52](https://github.com/sravioli/strata/issues/52)), and this design fixes their contract now.

## Motivation

The walking skeleton ([#49](https://github.com/sravioli/strata/issues/49)) needs a client to learn that an uploaded document can be searched. The processing of a revision happens in a job, after the upload ends ([#5](https://github.com/sravioli/strata/issues/5)). The time is not known, and it can be several seconds.

The known facts:

- Strata is a backend. Its clients are product backends. A frontend comes later, for demonstrations only.
- v1 has no message bus and no status cache ([#2](https://github.com/sravioli/strata/issues/2), [#5](https://github.com/sravioli/strata/issues/5)).
- At the design ceiling, clients upload 10,000 revisions each hour ([#2](https://github.com/sravioli/strata/issues/2)).

The owner asked which mechanism other services use. Services that process documents in the background give a status resource that the client reads again. Some also send a notification. The notification never replaces the status resource. See "Prior art".

## Guide-level explanation

### Terms

- **Public status**: the status of a revision that the API shows to a client. It is one of five values.
- **Final status**: a public status after which the processing of the revision stops. `ready`, `failed` and `superseded` are final.
- **Current revision**: the revision of a document that a search uses.

### The five public statuses

| Public status | Meaning | Final |
|---|---|---|
| `uploading` | The API gave a signed URL. The client did not finish the upload. | No |
| `processing` | The upload is complete. A worker did not finish the revision. | No |
| `ready` | The text of the revision is in the lexical index, and the revision is the current revision. | Yes |
| `failed` | The revision cannot be processed because of its file. The response gives the error class. | Yes |
| `superseded` | A newer revision of the same document is ready. A search does not use this revision. | Yes |

A `ready` revision becomes `superseded` when a newer revision of the same document becomes ready. A client that stops at `ready` does not see this change.

`failed` always means a cause in the file, for example `UnsupportedFormat` ([#7](https://github.com/sravioli/strata/issues/7)). A problem of Strata never shows as `failed`. If the job of a revision is `Parked`, the revision stays `processing`, and an operator requeues the job.

### Example: one upload

Alice uploads a PDF from the P1 backend.

1. The client starts the upload and gets a signed URL. The status is `uploading`.
2. The client sends the file and finishes the upload. The API answers `202 Accepted`, with a `Location` header: `/v1/documents/7b1e…/revisions/1`.
3. The client reads that URL. The answer is `processing`, with `Retry-After: 2`.
4. The client waits 2 seconds and reads again. The answer is `ready`, with no `Retry-After`.
5. The client searches, and the search finds the document.

### Example: two revisions

The client uploads revision 1, then revision 2 of the same document. Revision 2 becomes ready first. Revision 1 then becomes `superseded`. It never becomes `ready`, because an older revision never replaces a newer one ([#8](https://github.com/sravioli/strata/issues/8)).

### Example: many uploads

The P1 backend uploads 300 files. It reads the status of 100 revisions in one request, three times. It reads again only the revisions that are not final, after the `Retry-After` time.

## Reference-level explanation

### Status of one revision

`GET /v1/documents/{document_id}/revisions/{revision}`

The action is `ReadDocument` ([#4](https://github.com/sravioli/strata/issues/4)). The response body:

```json
{
  "document_id": "7b1e…",
  "revision": 1,
  "state": "failed",
  "error_class": "UnsupportedFormat",
  "updated_at": "2026-11-20T10:15:02Z"
}
```

- `error_class` exists only when `state` is `failed`. Its values are the classes of the extraction design ([#7](https://github.com/sravioli/strata/issues/7)).
- `updated_at` is the time of the last change of the public status.
- The API answers `404` in these cases. The `404` responses are equal ([#3](https://github.com/sravioli/strata/issues/3)).
  - The revision does not exist, or it belongs to a different tenant.
  - The cleanup removed an upload that the client did not finish.
  - An account removed the document.

### Batch status

`GET /v1/revisions?ids={document_id}:{revision},{document_id}:{revision},…`

- The action is `ReadDocument`.
- A request names 1 to 100 revisions. More than 100 gives `400`. A repeated identifier counts one time.
- The response has a list with one entry for each revision that the API found. Each entry has the fields of the single response.
- The list omits a revision that does not exist or that belongs to a different tenant. Thus the response does not show that a revision of a different tenant exists.
- With 100 identifiers, the URL is approximately 4 KB long.

### Finish upload

The "finish upload" step of the object storage design ([#6](https://github.com/sravioli/strata/issues/6)) answers `202 Accepted`. The `Location` header gives the URL of the status of that revision. A second "finish" gives the same answer.

### Map from internal states

| Internal state | Public status |
|---|---|
| Revision `AwaitingUpload` | `uploading` |
| Revision `Uploaded`, its job `Waiting`, `Running` or `Parked` | `processing` |
| Text in the lexical index, and the revision is the current revision | `ready` |
| Revision `Failed` | `failed` |
| A newer revision of the same document is ready | `superseded` |

More than one row can agree with a revision. Example: revision 1 is `processing` or `failed`, and revision 2 is ready. The API uses this order:

1. `failed` stays `failed`.
2. Otherwise, `superseded` comes before `uploading`, `processing` and `ready`.

For a `superseded` revision, `updated_at` is the time when the newer revision became ready.

The design of the walking skeleton ([#49](https://github.com/sravioli/strata/issues/49)) defines the revision lifecycle and its internal state names. This map then uses those names. The five public statuses do not change.

A revision with the class `NoText` is `ready` ([#7](https://github.com/sravioli/strata/issues/7)). See "Unresolved questions".

### Retry-After

- When the status is `uploading` or `processing`, the response has `Retry-After` in seconds. The value comes from the configuration. The default is 2.
- In a batch response, `Retry-After` exists when one revision or more in the list is not final.
- When all statuses are final, the response has no `Retry-After`.

`Retry-After` is the control of the polling load. Strata can make the value larger with no new version of the API.

### Limit on status reads

- The rule is for each revision: a client does not read the status of one revision more frequently than `Retry-After` permits.
- Each API instance keeps a token bucket in memory for each pair (account, revision). The two status routes use the same buckets.
- A bucket holds 3 tokens. It gets 1 token for each `Retry-After` interval. A read of one revision takes 1 token from its bucket. A batch request takes 1 token from the bucket of each revision that it names.
- If one bucket of a batch request is empty, the API answers `429` for the complete request.
- Each account also has one bucket for all its status requests. It holds 20 tokens and gets 10 tokens each second. Thus an account that names new revisions in each request still has a limit.

Example: a product backend uses one account and waits for 300 revisions. It sends 3 batch requests every 2 seconds. Each revision gets one read every 2 seconds, so no bucket becomes empty.

Other rules:

- An empty bucket gives `429 Too Many Requests`, with `Retry-After`.
- The number of buckets has a bound in the configuration. When the count reaches the bound, the API removes the bucket that it used least recently ([#2](https://github.com/sravioli/strata/issues/2), S3).
- With N instances, an account can read one revision up to N times the limit. The limit protects the API. It is not exact. A limit for all routes comes later ([#53](https://github.com/sravioli/strata/issues/53)).

### Reads from a replica

The status routes are document read routes, so they can read from a replica ([#2](https://github.com/sravioli/strata/issues/2), S2). A search also reads from a replica. A replica that shows `ready` can find the document in a search. A different replica can be some seconds behind. The API documentation says that a search can miss a `ready` revision for some seconds.

The walking skeleton has no replica.

### Load at the ceiling

Assume that each client reads every 2 seconds and that processing takes 60 seconds. Each upload then gives 30 reads. 10,000 uploads each hour give approximately 83 reads each second. The ceiling is 1,000 searches each second ([#2](https://github.com/sravioli/strata/issues/2)). Thus status reads add less than 10 % to the read load. Each read is one read by primary key.

### Contract for webhooks

Webhooks come later ([#52](https://github.com/sravioli/strata/issues/52)). This design fixes their contract, so that the webhooks do not change the API of v1:

- The events are `revision.ready`, `revision.failed` and `revision.superseded`.
- The payload holds only `document_id`, `revision` and `state`. It holds no content and no error detail. The client reads the status resource for the rest.
- Strata signs each message with the scheme of Standard Webhooks.
- Delivery is at least once, and the order is not guaranteed. A client must accept a repeated event.
- The status resource stays the source of truth.

## Proof obligations

N/A. This design is an HTTP interface and a map from internal states. The rules of the revision state machine belong to the design of the revision lifecycle ([#49](https://github.com/sravioli/strata/issues/49)) and to spike [#8](https://github.com/sravioli/strata/issues/8). The L0 and L1 test cases below cover this design.

## Impacts

### Infrastructure

- No new service, no new storage and no added cost.
- Metrics:
  - the count of status reads, for each route,
  - the count of `429` responses,
  - the count of token buckets in each instance.
- Rollout: this design is part of the first API. There is no earlier state to roll back to.

### Security and tenant isolation

- The two routes use `ReadDocument` and a tenant transaction ([#3](https://github.com/sravioli/strata/issues/3), [#4](https://github.com/sravioli/strata/issues/4)).
- A revision of a different tenant gives the same `404` as a revision that does not exist. A batch request omits it.
- `error_class` comes from a fixed list. It never holds an error text, because a text can hold tenant content.
- New untrusted input: the identifiers in the path and in `ids`. The API parses each one as a UUID and a positive integer, and rejects a request with an identifier that fails.

### Data and migration

No schema change. The routes read the revision and its job.

## Test cases

L0: the map from internal states to public statuses, the parse of `ids`, and the token bucket.

| ID | Level | Given, when, then | Decided by | Automated test |
|---|---|---|---|---|
| TC-47-01 | L1 | Given a revision in each internal state of the map. When a client reads its status. Then the public status is the status in the map. | | pending |
| TC-47-02 | L1 | Given a revision whose job is `Parked`. When a client reads its status. Then the status is `processing`. | | pending |
| TC-47-03 | L1 | Given revision 1 and revision 2 of one document, and revision 2 is ready. When a client reads revision 1. Then the status is `superseded`. | | pending |
| TC-47-04 | L1 | Given a revision that is not final. When a client reads its status. Then the response has `Retry-After` with the configured value. Given a revision that is final. Then the response has no `Retry-After`. | | pending |
| TC-47-05 | L1 | Given an account that read the status of revision R 3 times in less than one `Retry-After` interval. When it reads R again. Then the API answers `429` with `Retry-After`. When it reads a different revision. Then the API answers with its status. | | pending |
| TC-47-12 | L1 | Given one account that waits for 300 revisions. When it sends 3 batch requests of 100 revisions in each `Retry-After` interval, for 10 intervals. Then no response is `429`. | | pending |
| TC-47-13 | L1 | Given revision 1 is `failed` and revision 2 of the same document is ready. When a client reads revision 1. Then the status is `failed`. Given revision 1 is `processing` and revision 2 is ready. Then the status of revision 1 is `superseded`. | | pending |
| TC-47-14 | L1 | Given an account that sends 21 status requests in less than one second, each for a different revision. When it sends one more. Then the API answers `429`. | | pending |
| TC-47-06 | L1 | Given a revision of tenant B. When an account of tenant A reads its status. Then the response is equal to the response for a revision that does not exist. When the account names it in a batch request. Then the list omits it. | | pending |
| TC-47-07 | L1 | Given a batch request with 101 identifiers, or with an identifier that is not valid. When a client sends it. Then the API answers `400`. | | pending |
| TC-47-08 | L1 | Given a batch request with one revision that is `processing` and one that is `ready`. When a client sends it. Then the response has two entries and `Retry-After`. | | pending |
| TC-47-09 | L1 | Given an upload that the client finished. When the client finishes it a second time. Then each answer is `202` with the same `Location`. | | pending |
| TC-47-10 | L1 | Given a revision that is `failed`. When a client reads its status. Then `error_class` is a value from the list of the extraction design, and the body holds no other error text. | | pending |
| TC-47-11 | L2 | Given the walking skeleton on a laptop. When a client uploads a document and reads the `Location` URL until the status is final. Then the status is `ready`, and a search finds the document. | | pending |

L3: none in this design. TC-2-01 of the capacity design measures the read load. A later L3 test case can add status reads to that load.

## Drawbacks

- A client polls. A notification is faster and costs less for one waiting client.
- The limit on status reads is not exact with more than one instance.
- A `ready` revision can become `superseded` with no notice in v1.

## Rationale and alternatives

**Webhooks in v1.** The owner leaned to this option. Webhooks need an outbox, signatures, retries, protection against server-side request forgery, and the management of URLs. The walking skeleton is the shortest path from upload to search, and each of these parts adds to it. A webhook is also not reliable alone, so the status resource must exist. The owner chose polling first and webhooks later ([#52](https://github.com/sravioli/strata/issues/52)) on 2026-11-20.

**Server-sent events.** Each waiting client keeps one connection open. With no message bus, the API must read the database to know when to send. The clients are backends, not browsers. Rejected for v1, and tracked for browser clients ([#55](https://github.com/sravioli/strata/issues/55)).

**Show internal states to clients.** This needs no map. But each change of the revision lifecycle then breaks clients. Rejected.

**A fifth status `stalled` for a `Parked` job.** This shows an operator task to the client, and the job can continue after a requeue. Rejected.

**Status from the primary.** The status then shows `ready` sooner. But a search on a replica that is behind does not find the revision. Rejected.

**A shared counter for the limit.** This is exact. It needs one write for each read, or a new service. Rejected for v1. A limit for all routes comes later ([#53](https://github.com/sravioli/strata/issues/53)).

**A batch read by filter, for example all revisions that are `processing`.** The client then does not keep identifiers, but the response needs pages. Rejected for v1, and tracked ([#54](https://github.com/sravioli/strata/issues/54)).

## Prior art

- RFC 9110, "HTTP Semantics": `202 Accepted` (section 15.3.3) and `Retry-After` (section 10.2.3). https://www.rfc-editor.org/rfc/rfc9110
- RFC 6585, "Additional HTTP Status Codes": `429 Too Many Requests`. https://www.rfc-editor.org/rfc/rfc6585
- Google AIP-151, "Long-running operations". A client reads an operation resource until it is done. https://google.aip.dev/151
- Azure AI Document Intelligence. An analysis answers `202` with an `Operation-Location` header, and the client reads that URL. https://learn.microsoft.com/en-us/rest/api/aiservices/document-models/analyze-document
- Amazon Textract, "Calling Amazon Textract Asynchronous Operations". A job sends a notification through Amazon SNS, and the client then gets the result with a second call. https://docs.aws.amazon.com/textract/latest/dg/api-async.html
- Standard Webhooks specification. https://github.com/standard-webhooks/standard-webhooks
- OWASP, "Webhook Security Cheat Sheet". https://cheatsheetseries.owasp.org/cheatsheets/Webhook_Security_Cheat_Sheet.html

## Unresolved questions

The owner must decide:

- The test cases above.
- If a `ready` revision with the class `NoText` shows that class to the client. A client cannot see now that the revision has no text. A marker adds a field to the public contract.

Implementation resolves these points:

- The internal state names of the revision lifecycle, in the design of [#49](https://github.com/sravioli/strata/issues/49).
- The configured bound on the number of token buckets.

## Future possibilities

Each item has an issue:

- Webhooks for revision status: [#52](https://github.com/sravioli/strata/issues/52).
- Rate limits for each account on all routes: [#53](https://github.com/sravioli/strata/issues/53).
- A revision list filtered by state: [#54](https://github.com/sravioli/strata/issues/54).
- Status push to browser clients: [#55](https://github.com/sravioli/strata/issues/55).
