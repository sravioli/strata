-- The tables of tech design 0049 that the four shapes touch, with the policy of 0003.
create extension btree_gin;
-- run.sh sets the password from BENCH_PASSWORD (see README).
create role bench_app login nobypassrls;

create table documents (
  tenant_id uuid not null,
  document_id uuid not null default uuidv7(),
  last_revision int not null,
  ready_revision int,
  created_at timestamptz not null default now(),
  primary key (tenant_id, document_id)
);

create table revisions (
  tenant_id uuid not null,
  document_id uuid not null,
  revision int not null,
  state text not null,
  file_name text not null,
  content_type text not null,
  size bigint not null,
  retry_mark bool not null default false,
  error_class text,
  no_text bool not null default false,
  status_at timestamptz not null default now(),
  created_at timestamptz not null default now(),
  primary key (tenant_id, document_id, revision),
  foreign key (tenant_id, document_id) references documents
);

create table text_parts (
  tenant_id uuid not null,
  document_id uuid not null,
  revision int not null,
  part int not null,
  location_kind text not null,
  location int not null,
  text text not null,
  tsv tsvector generated always as (to_tsvector('simple', text)) stored,
  primary key (tenant_id, document_id, revision, part),
  foreign key (tenant_id, document_id, revision) references revisions
);
create index on text_parts using gin (tenant_id, tsv);

do $$
declare t text;
begin
  foreach t in array array['documents', 'revisions', 'text_parts'] loop
    execute format('alter table %I enable row level security', t);
    execute format('alter table %I force row level security', t);
    execute format($p$create policy tenant on %I
      using      (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
      with check (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)$p$, t);
  end loop;
end $$;

grant select, insert, update on documents, revisions to bench_app;
grant select, insert on text_parts to bench_app;
