-- Tenants and accounts (tech design 0003), issuers and issuer bindings
-- (tech design 0021). strata_owner applies this file (`strata migrate`).
-- The roles come from dev/roles.sql.

create table tenants (
    tenant_id uuid primary key default uuidv7(),
    -- The column name of tech design 0003.
    name text not null unique  -- noqa: RF04
    check (name ~ '^[a-z][a-z0-9-]{0,62}$'),
    status text not null default 'active'
    check (status in ('active', 'not-active')),
    created_at timestamptz not null default now()
);

alter table tenants enable row level security;
alter table tenants force row level security;
create policy tenants_select on tenants for select
using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

-- A global table: settings, no tenant content.
create table issuers (
    issuer_id uuid primary key default uuidv7(),
    issuer text not null unique,
    jwks_url text not null,
    audience text not null,
    algorithm text not null check (algorithm in ('RS256', 'ES256', 'EdDSA')),
    tenant_claim text not null,
    account_claim text not null default 'sub',
    max_lifetime_s integer not null default 3600
    check (max_lifetime_s between 1 and 3600),
    status text not null default 'active'
    check (status in ('active', 'not-active'))
);

create table issuer_bindings (
    issuer_id uuid not null references issuers (issuer_id),
    tenant_value text not null
    check (octet_length(tenant_value) between 1 and 256),
    tenant_id uuid not null references tenants (tenant_id),
    primary key (issuer_id, tenant_value)
);

alter table issuer_bindings enable row level security;
alter table issuer_bindings force row level security;
create policy issuer_bindings_select on issuer_bindings for select
using (
    issuer_id = nullif(current_setting('app.issuer_id', true), '')::uuid
    and tenant_value = nullif(current_setting('app.tenant_value', true), '')
);

create table accounts (
    tenant_id uuid not null references tenants (tenant_id),
    account_id uuid not null default uuidv7(),
    issuer_id uuid not null references issuers (issuer_id),
    subject text not null check (octet_length(subject) between 1 and 256),
    status text not null default 'active'
    check (status in ('active', 'not-active')),
    created_at timestamptz not null default now(),
    primary key (tenant_id, account_id),
    unique (tenant_id, issuer_id, subject)
);

alter table accounts enable row level security;
alter table accounts force row level security;
create policy accounts_tenant on accounts
using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
with check (
    tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid
);

-- The grants of the spec tenant-isolation. An L1 test reads them from the
-- catalog.
grant select on tenants, issuers, issuer_bindings to strata_api;
grant select, insert on accounts to strata_api;
grant select on tenants to strata_worker;
grant select, insert, update on tenants, issuers to strata_operator;
grant select, insert, delete on issuer_bindings to strata_operator;
grant select, update on accounts to strata_operator;
