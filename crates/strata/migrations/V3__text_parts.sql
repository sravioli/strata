-- The text of each Ready revision (tech design 0049). strata_owner applies
-- this file (`strata migrate`). btree_gin is a trusted extension, and
-- strata_owner owns the database.

create extension if not exists btree_gin;

create table text_parts (
    tenant_id uuid not null,
    document_id uuid not null,
    revision integer not null,
    part integer not null check (part >= 1),
    location_kind text not null
    check (location_kind in ('page', 'sheet', 'slide')),
    location_number integer not null check (location_number >= 1),
    part_text text not null check (octet_length(part_text) <= 65536),
    tsv tsvector generated always as (to_tsvector('simple', part_text)) stored,
    primary key (tenant_id, document_id, revision, part),
    foreign key (tenant_id, document_id, revision)
    references revisions (tenant_id, document_id, revision)
);

alter table text_parts enable row level security;
alter table text_parts force row level security;
create policy text_parts_tenant on text_parts
using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
with check (
    tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid
);

-- A search reads only the index entries of one tenant (S5 of 0002).
create index text_parts_tsv on text_parts using gin (tenant_id, tsv);

-- The grants of the spec tenant-isolation. An L1 test reads them from the
-- catalog.
grant select, insert, delete on text_parts to strata_worker;
