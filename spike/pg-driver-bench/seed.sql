-- 20 tenants x 250 documents x 20 pages. A page has 300 words from a vocabulary of 10,000,
-- so one word is in about 3 % of the pages: about 150 matches for each tenant.
insert into documents (tenant_id, last_revision, ready_revision)
select t.id, 1, 1
from (select uuidv7() as id from generate_series(1, 20)) as t, generate_series(1, 250);

insert into revisions (tenant_id, document_id, revision, state, file_name, content_type, size)
select tenant_id, document_id, 1, 'Ready', 'seed.pdf', 'application/pdf', 1000 from documents;

insert into text_parts (tenant_id, document_id, revision, part, location_kind, location, text)
select d.tenant_id, d.document_id, 1, p, 'page', p,
       -- The reference to d and p makes PostgreSQL run the subquery again for each row.
       (select string_agg('w' || (1 + floor(random() * 10000))::int, ' ')
        from generate_series(1, 300) where p > 0 and d.document_id is not null)
from documents d, generate_series(1, 20) as p;

vacuum analyze;
