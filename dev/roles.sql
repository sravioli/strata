-- The four roles of tech design 0003. The superuser runs this file
-- once, before the migrations: compose.yaml at the first start of a new
-- data folder, and `make sql`.
create role strata_owner login;
create role strata_api login
nosuperuser nocreatedb nocreaterole noinherit nobypassrls;
create role strata_worker login
nosuperuser nocreatedb nocreaterole noinherit nobypassrls;
create role strata_operator login
nosuperuser nocreatedb nocreaterole noinherit bypassrls;

-- Development only: each role logs in with its own name as the secret.
do $$
declare
  r text;
begin
  foreach r in array array[
    'strata_owner', 'strata_api', 'strata_worker', 'strata_operator'
  ] loop
    execute format('alter role %I password %L', r, r);
  end loop;
end
$$;

-- The owner role owns the database, so it gets CREATE on the schema
-- public (PostgreSQL 15 and later).
do $$
begin
  execute format(
    'alter database %I owner to strata_owner', current_database()
  );
end
$$;

-- The match of a full-text query (`tsv @@ query`) reveals nothing of a row
-- other than its result. Without this mark, PostgreSQL never uses the GIN
-- index of text_parts under the policy of tech design 0003 (tech design 0049,
-- owner 2026-11-22). Only a superuser can set it, in each database.
alter function ts_match_vq(tsvector, tsquery) leakproof;
