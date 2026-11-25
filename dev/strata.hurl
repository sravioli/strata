# The walking skeleton end to end: upload a PDF, wait until it is ready, read the status, search.
# Run `make dev` first, then `make hurl` from the repo root. It sets the variables base_url, acme,
# globex and size, and runs `hurl --test --file-root .` on this file. The manual check TC-133-07
# (scripts/observability-check.sh) needs one run.
#
# Run `hurl` without --test to see the last response body. The text of the fixture is "alpha page
# one" and "bravo page two". For another PDF, change the file below, the size, and the word in the
# search.

# 1. Start a document.
POST {{base_url}}/v1/documents
Authorization: Bearer {{acme}}
{
  "file_name": "two-pages.pdf",
  "content_type": "application/pdf",
  "size": {{size}}
}
HTTP 201
[Captures]
document_id: jsonpath "$.document_id"
revision: jsonpath "$.revision"
upload_url: jsonpath "$.upload_url"

# 2. Upload the file to the signed URL. No bearer token: the URL carries the signature.
PUT {{upload_url}}
Content-Type: application/pdf
file,crates/strata-tests/fixtures/two-pages.pdf;
HTTP 200

# 3. Finish the upload.
POST {{base_url}}/v1/documents/{{document_id}}/revisions/{{revision}}/finish
Authorization: Bearer {{acme}}
HTTP 202

# 4. Read the status until the worker has extracted the text. The interval is the Retry-After of 2 s,
# so the bucket of the revision (3 tokens) does not run empty.
GET {{base_url}}/v1/documents/{{document_id}}/revisions/{{revision}}
Authorization: Bearer {{acme}}
[Options]
retry: 15
retry-interval: 2000ms
HTTP 200
[Asserts]
header "Cache-Control" == "no-store"
jsonpath "$.state" == "ready"
jsonpath "$.no_text" == false

# 5. The status of the revision with the batch route.
QUERY {{base_url}}/v1/revisions
Authorization: Bearer {{acme}}
[Options]
delay: 2000ms
{
  "revisions": [{"document_id": "{{document_id}}", "revision": {{revision}}}]
}
HTTP 200
[Asserts]
header "Accept-Query" == "application/json"
jsonpath "$.revisions[0].state" == "ready"

# 6. Search: the new document is in the results of acme.
QUERY {{base_url}}/v1/search
Authorization: Bearer {{acme}}
{
  "q": "alpha",
  "limit": 100
}
HTTP 200
[Asserts]
jsonpath "$.results[*].document_id" contains "{{document_id}}"
jsonpath "$.results[0].text[?(@.match == true)].t" contains "alpha"

# 7. The same search as globex does not find it.
QUERY {{base_url}}/v1/search
Authorization: Bearer {{globex}}
{
  "q": "alpha",
  "limit": 100
}
HTTP 200
[Asserts]
jsonpath "$.results[*].document_id" not contains "{{document_id}}"

# 8. A status of a revision of acme, read as globex, is a 404.
GET {{base_url}}/v1/documents/{{document_id}}/revisions/{{revision}}
Authorization: Bearer {{globex}}
HTTP 404
