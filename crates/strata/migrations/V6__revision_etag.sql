-- The ETag of the object that the finish checked (tech design 0006, amended
-- for the audit of 2026-11-26). The worker reads only the object of that
-- ETag, so a PUT after the finish never reaches the corpus. Null before the
-- finish. strata_owner applies this file (`strata migrate`).

alter table revisions add column etag text
check (char_length(etag) between 1 and 1024);
