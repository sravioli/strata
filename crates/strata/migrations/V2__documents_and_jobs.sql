-- Documents and revisions (tech design 0049), the job queue (tech design 0005)
-- and the bound on unfinished uploads (tech design 0006). strata_owner applies
-- this file (`strata migrate`).

create table documents (
    tenant_id uuid not null references tenants (tenant_id),
    document_id uuid not null default uuidv7(),
    last_revision integer not null check (last_revision >= 1),
    ready_revision integer check (ready_revision between 1 and last_revision),
    created_at timestamptz not null default now(),
    primary key (tenant_id, document_id)
);

alter table documents enable row level security;
alter table documents force row level security;
create policy documents_tenant on documents
using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
with check (
    tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid
);

create table revisions (
    tenant_id uuid not null,
    document_id uuid not null,
    revision integer not null check (revision >= 1),
    state text not null default 'AwaitingUpload'
    check (
        state in (
            'AwaitingUpload', 'Uploaded', 'Ready', 'Failed', 'Superseded'
        )
    ),
    file_name text not null check (char_length(file_name) between 1 and 1024),
    content_type text not null
    check (char_length(content_type) between 1 and 1024),
    size bigint not null check (size between 0 and 104857600),
    retry_mark boolean not null default false,
    error_class text
    check (
        error_class in (
            'FileTooLarge',
            'ExtractionTimeout',
            'ExtractionMemory',
            'ExtractionCrash',
            'ExtractionOutputTooLarge',
            'ArchiveTooLarge',
            'UnsupportedFormat',
            'EncryptedFile',
            'MalformedFile',
            'InvalidOutput'
        )
    ),
    no_text boolean not null default false,
    -- The key of the row of upload_slots (a version 4 UUID, owner 2026-11-22).
    slot_id uuid not null,
    status_at timestamptz not null default now(),
    created_at timestamptz not null default now(),
    primary key (tenant_id, document_id, revision),
    foreign key (tenant_id, document_id)
    references documents (tenant_id, document_id),
    -- A class only for a Failed revision.
    check ((state = 'Failed') = (error_class is not null))
);

alter table revisions enable row level security;
alter table revisions force row level security;
create policy revisions_tenant on revisions
using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
with check (
    tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid
);

-- A global table: a random key and a time, no tenant content. The start
-- inserts a row, and the finish removes it by its key.
create table upload_slots (
    slot_id uuid primary key default gen_random_uuid(),
    created_at timestamptz not null default now()
);

create index upload_slots_created_at on upload_slots (created_at);

-- A global table: identifiers, a state and an error class (tech design 0005).
-- The target has no foreign key: revisions is tenant-owned.
create table jobs (
    job_id uuid primary key default uuidv7(),
    tenant_id uuid not null,
    kind text not null check (kind in ('ProcessDocument', 'RemoveFiles')),
    document_id uuid not null,
    revision integer,
    state text not null default 'Waiting'
    check (state in ('Waiting', 'Running', 'Parked', 'Done')),
    attempts integer not null default 0 check (attempts >= 0),
    run_after timestamptz not null default now(),
    lease_until timestamptz,
    lease_token uuid,
    ended_at timestamptz,
    last_error text not null default 'NoError'
    check (
        last_error in (
            'NoError', 'TenantNotActive', 'LeaseExpired', 'JobFailed'
        )
    )
);

create index jobs_waiting on jobs (run_after) where state = 'Waiting';
create index jobs_running on jobs (lease_until) where state = 'Running';
create index jobs_done on jobs (ended_at) where state = 'Done';

-- The grants of the spec tenant-isolation. An L1 test reads them from the
-- catalog.
grant select, insert, update on documents, revisions to strata_api;
grant insert on jobs to strata_api;
grant select (state) on jobs to strata_api;
grant insert, delete on upload_slots to strata_api;
grant select (slot_id, created_at) on upload_slots to strata_api;
grant select on documents to strata_worker;
grant update (ready_revision) on documents to strata_worker;
grant select, update on revisions to strata_worker;
grant select, update, delete on jobs to strata_worker;
grant select, update on jobs to strata_operator;
