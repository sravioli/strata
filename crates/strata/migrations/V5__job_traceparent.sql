-- The trace context of the request that queued a job (tech design 0133). Null,
-- or a W3C Trace Context value: version 00, a trace ID of 32 lowercase hex
-- digits, a span ID of 16 and flags of 2. strata_owner applies this file
-- (`strata migrate`).

alter table jobs add column traceparent text
check (traceparent ~ '^00-[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{2}$');
