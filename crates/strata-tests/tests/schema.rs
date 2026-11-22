//! L1 tests of the schema: the migrations, the catalog, the policies and the grants
//! (specs schema-migrations and tenant-isolation).

mod common;

use std::collections::BTreeSet;

use common::*;
use strata::db::testing::tenant_tx;
use tokio_postgres::Client;
use uuid::Uuid;

/// Tables that have no row-level security and hold no tenant content (tech design 0003).
const GLOBAL_TABLES: &[&str] = &["issuers", "refinery_schema_history"];

/// The policy expressions, copied from tech designs 0003 and 0021 with no change (TC-3-06).
const TENANT_EXPR: &str = "tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid";
const BINDING_EXPR: &str = "issuer_id = nullif(current_setting('app.issuer_id', true), '')::uuid
  and tenant_value = nullif(current_setting('app.tenant_value', true), '')";

/// The grants of the spec tenant-isolation: (role, table, privilege).
const GRANTS: &[(&str, &str, &str)] = &[
    ("strata_api", "tenants", "SELECT"),
    ("strata_api", "issuers", "SELECT"),
    ("strata_api", "issuer_bindings", "SELECT"),
    ("strata_api", "accounts", "SELECT"),
    ("strata_api", "accounts", "INSERT"),
    ("strata_worker", "tenants", "SELECT"),
    ("strata_operator", "tenants", "SELECT"),
    ("strata_operator", "tenants", "INSERT"),
    ("strata_operator", "tenants", "UPDATE"),
    ("strata_operator", "issuers", "SELECT"),
    ("strata_operator", "issuers", "INSERT"),
    ("strata_operator", "issuers", "UPDATE"),
    ("strata_operator", "issuer_bindings", "SELECT"),
    ("strata_operator", "issuer_bindings", "INSERT"),
    ("strata_operator", "issuer_bindings", "DELETE"),
    ("strata_operator", "accounts", "SELECT"),
    ("strata_operator", "accounts", "UPDATE"),
];

const RUNTIME_ROLES: &[&str] = &["strata_api", "strata_worker"];

async fn tables(c: &Client) -> Vec<String> {
    c.query(
        "select relname::text from pg_class where relnamespace = 'public'::regnamespace \
         and relkind in ('r', 'p') order by 1",
        &[],
    )
    .await
    .unwrap()
    .iter()
    .map(|r| r.get(0))
    .collect()
}

/// The text that PostgreSQL gives for an expression, from a policy on a temporary table.
async fn deparse(c: &Client, expr: &str) -> String {
    let t = unique("expr").replace('-', "_");
    c.batch_execute(&format!(
        "create temporary table {t} (tenant_id uuid, issuer_id uuid, tenant_value text); \
         create policy p on {t} using ({expr});"
    ))
    .await
    .unwrap();
    c.query_one("select qual from pg_policies where tablename = $1", &[&t])
        .await
        .unwrap()
        .get(0)
}

#[tokio::test]
async fn migrations_on_an_empty_database_then_again_then_changed() {
    migrated();
    let su = connect(&superuser()).await;
    let name = unique("strata_mig").replace('-', "_");
    su.batch_execute(&format!("create database {name} owner strata_owner"))
        .await
        .unwrap();
    let mut owner = config("strata_owner", POSTGRES);
    owner.dbname(&name);

    // First run on an empty database: the tables exist, and strata_owner owns each one.
    strata::migrate::run(&owner).await.unwrap();
    let c = connect(&owner).await;
    let owners: Vec<(String, String)> = c
        .query(
            "select relname::text, relowner::regrole::text from pg_class \
             where relnamespace = 'public'::regnamespace and relkind = 'r'",
            &[],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    for t in ["tenants", "accounts", "issuers", "issuer_bindings"] {
        assert!(
            owners.contains(&(t.into(), "strata_owner".into())),
            "{t}: {owners:?}"
        );
    }

    // Second run: it succeeds and changes nothing.
    let count = "select count(*) from refinery_schema_history";
    let before: i64 = c.query_one(count, &[]).await.unwrap().get(0);
    strata::migrate::run(&owner).await.unwrap();
    assert_eq!(
        c.query_one(count, &[]).await.unwrap().get::<_, i64>(0),
        before
    );

    // A different text of an applied migration stops the run.
    c.execute(
        "update refinery_schema_history set checksum = '1' where version = 1",
        &[],
    )
    .await
    .unwrap();
    assert!(strata::migrate::run(&owner).await.is_err());
    drop(c);
    su.batch_execute(&format!("drop database {name} with (force)"))
        .await
        .unwrap();
}

/// TC-3-06
#[tokio::test]
async fn each_table_has_its_policy_or_is_global() {
    migrated();
    let c = connect(&superuser()).await;
    let tenant_expr = deparse(&c, TENANT_EXPR).await;
    let binding_expr = deparse(&c, BINDING_EXPR).await;
    for t in tables(&c).await {
        if GLOBAL_TABLES.contains(&t.as_str()) {
            continue;
        }
        let row = c
            .query_one(
                "select relrowsecurity, relforcerowsecurity from pg_class \
                 where oid = ('public.' || $1)::regclass",
                &[&t],
            )
            .await
            .unwrap();
        assert!(
            row.get::<_, bool>(0) && row.get::<_, bool>(1),
            "{t}: forced row-level security"
        );
        let policies = c
            .query(
                "select cmd, qual, with_check, permissive from pg_policies \
                 where schemaname = 'public' and tablename = $1",
                &[&t],
            )
            .await
            .unwrap();
        assert_eq!(policies.len(), 1, "{t}: exactly one policy");
        let p = &policies[0];
        let (cmd, qual, check): (String, Option<String>, Option<String>) =
            (p.get(0), p.get(1), p.get(2));
        assert_eq!(p.get::<_, String>(3), "PERMISSIVE", "{t}");
        match t.as_str() {
            "tenants" => assert_eq!(
                (cmd.as_str(), qual, check),
                ("SELECT", Some(tenant_expr.clone()), None)
            ),
            "issuer_bindings" => {
                assert_eq!(
                    (cmd.as_str(), qual, check),
                    ("SELECT", Some(binding_expr.clone()), None)
                )
            }
            _ => {
                assert_eq!(cmd, "ALL", "{t}");
                assert_eq!(qual.as_ref(), Some(&tenant_expr), "{t}");
                assert_eq!(check.as_ref(), Some(&tenant_expr), "{t}");
                let has_tenant: bool = c
                    .query_one(
                        "select exists (select from information_schema.columns where table_schema = 'public' \
                         and table_name = $1 and column_name = 'tenant_id' and is_nullable = 'NO')",
                        &[&t],
                    )
                    .await
                    .unwrap()
                    .get(0);
                assert!(has_tenant, "{t}: a tenant_id column that is not null");
            }
        }
    }
}

/// TC-3-07
#[tokio::test]
async fn runtime_roles_have_only_limited_rights() {
    migrated();
    let c = connect(&superuser()).await;
    for role in RUNTIME_ROLES {
        let r = c
            .query_one(
                "select rolsuper, rolbypassrls, rolcreaterole, rolcreatedb, \
                 has_schema_privilege($1, 'public', 'CREATE'), \
                 has_database_privilege($1, current_database(), 'CREATE'), \
                 (select count(*) from pg_class where relowner = $1::regrole), \
                 (select count(*) from pg_auth_members where member = $1::regrole) \
                 from pg_roles where rolname = $1",
                &[role],
            )
            .await
            .unwrap();
        for (i, what) in [
            "superuser",
            "bypassrls",
            "createrole",
            "createdb",
            "create in public",
            "create schema",
        ]
        .iter()
        .enumerate()
        {
            assert!(!r.get::<_, bool>(i), "{role}: {what}");
        }
        assert_eq!(r.get::<_, i64>(6), 0, "{role} owns an object");
        assert_eq!(r.get::<_, i64>(7), 0, "{role} is a member of a role");
        for t in tables(&c).await {
            let truncate: bool = c
                .query_one(
                    "select has_table_privilege($1, 'public.' || $2, 'TRUNCATE')",
                    &[role, &t],
                )
                .await
                .unwrap()
                .get(0);
            assert!(!truncate, "{role} can truncate {t}");
        }
    }
}

#[tokio::test]
async fn grants_are_exactly_the_list() {
    migrated();
    let c = connect(&superuser()).await;
    let rows = c
        .query(
            "select a.grantee::regrole::text, c.relname::text, a.privilege_type \
             from pg_class c, aclexplode(c.relacl) a \
             where c.relnamespace = 'public'::regnamespace \
             and a.grantee::regrole::text in ('strata_api', 'strata_worker', 'strata_operator')",
            &[],
        )
        .await
        .unwrap();
    let got: BTreeSet<(String, String, String)> = rows
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get(2)))
        .collect();
    let want: BTreeSet<(String, String, String)> = GRANTS
        .iter()
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
        .collect();
    assert_eq!(got, want);
    let columns: i64 = c
        .query_one(
            "select count(*) from pg_attribute a join pg_class c on c.oid = a.attrelid, aclexplode(a.attacl) x \
             where c.relnamespace = 'public'::regnamespace \
             and x.grantee::regrole::text in ('strata_api', 'strata_worker', 'strata_operator')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(columns, 0, "column grants that the list does not give");
}

struct Two {
    su: Client,
    a: Uuid,
    b: Uuid,
    issuer: Uuid,
}

async fn two_tenants() -> Two {
    migrated();
    let su = connect(&superuser()).await;
    let (a, b) = (tenant(&su).await, tenant(&su).await);
    let (issuer, _) = issuer(&su, "https://p.example/jwks").await;
    account(&su, a, issuer).await;
    account(&su, b, issuer).await;
    Two { su, a, b, issuer }
}

/// TC-3-01
#[tokio::test]
async fn select_with_no_filter_gives_only_the_rows_of_the_tenant() {
    let t = two_tenants().await;
    let tx = tenant_tx(&api_pool(), t.a).await.unwrap();
    let tenants: BTreeSet<Uuid> = tx
        .client()
        .query("select tenant_id from accounts", &[])
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(tenants, BTreeSet::from([t.a]));
    drop(t.su);
}

/// TC-3-02
#[tokio::test]
async fn no_tenant_context_gives_no_rows_and_no_writes() {
    let t = two_tenants().await;
    let api = connect(&config("strata_api", PGBOUNCER)).await;
    assert!(
        api.query("select * from accounts", &[])
            .await
            .unwrap()
            .is_empty()
    );
    let insert = api
        .execute(
            "insert into accounts (tenant_id, issuer_id, subject) values ($1, $2, 'x')",
            &[&t.a, &t.issuer],
        )
        .await;
    assert_eq!(
        sqlstate(insert),
        "42501",
        "row-level security rejects the insert"
    );
}

/// TC-3-03
#[tokio::test]
async fn insert_with_the_tenant_of_a_different_tenant_fails() {
    let t = two_tenants().await;
    let tx = tenant_tx(&api_pool(), t.a).await.unwrap();
    let insert = tx
        .client()
        .execute(
            "insert into accounts (tenant_id, issuer_id, subject) values ($1, $2, 'x')",
            &[&t.b, &t.issuer],
        )
        .await;
    assert_eq!(sqlstate(insert), "42501");
}

/// TC-3-15. `strata_api` has no update on `accounts`, so the policy test uses the owner role.
/// Row-level security is forced, so the policy applies to the owner too.
#[tokio::test]
async fn update_of_the_tenant_of_a_row_fails() {
    let t = two_tenants().await;
    let owner = connect(&config("strata_owner", POSTGRES)).await;
    owner.batch_execute("begin").await.unwrap();
    owner
        .query_one(
            "select set_config('app.tenant_id', $1, true)",
            &[&t.a.to_string()],
        )
        .await
        .unwrap();
    let n = owner
        .execute("update accounts set tenant_id = $1", &[&t.b])
        .await;
    assert_eq!(sqlstate(n), "42501");
    owner.batch_execute("rollback").await.unwrap();

    let tx = tenant_tx(&api_pool(), t.a).await.unwrap();
    let n = tx
        .client()
        .execute("update accounts set tenant_id = $1", &[&t.b])
        .await;
    assert_eq!(sqlstate(n), "42501", "strata_api has no update on accounts");
}

/// TC-3-20
#[tokio::test]
async fn tenants_table_shows_only_the_tenant_of_the_context() {
    let t = two_tenants().await;
    let tx = tenant_tx(&api_pool(), t.a).await.unwrap();
    let ids: Vec<Uuid> = tx
        .client()
        .query("select tenant_id from tenants", &[])
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(ids, [t.a]);
    let api = connect(&config("strata_api", PGBOUNCER)).await;
    assert!(
        api.query("select * from tenants", &[])
            .await
            .unwrap()
            .is_empty()
    );
}

/// TC-21-15
#[tokio::test]
async fn a_runtime_role_cannot_list_the_bindings() {
    let t = two_tenants().await;
    let (p2, _) = issuer(&t.su, "https://p2.example/jwks").await;
    bind(&t.su, t.issuer, "acme-corp", t.a).await;
    bind(&t.su, p2, "t-2", t.b).await;
    let api = connect(&config("strata_api", PGBOUNCER)).await;
    assert!(
        api.query("select * from issuer_bindings", &[])
            .await
            .unwrap()
            .is_empty()
    );
    api.batch_execute("begin").await.unwrap();
    api.query_one(
        "select set_config('app.issuer_id', $1, true)",
        &[&t.issuer.to_string()],
    )
    .await
    .unwrap();
    assert!(
        api.query("select * from issuer_bindings", &[])
            .await
            .unwrap()
            .is_empty()
    );
    // With both settings, the one binding shows.
    api.query_one(
        "select set_config('app.tenant_value', 'acme-corp', true)",
        &[],
    )
    .await
    .unwrap();
    assert_eq!(
        api.query("select * from issuer_bindings", &[])
            .await
            .unwrap()
            .len(),
        1
    );
    api.batch_execute("rollback").await.unwrap();
}

#[tokio::test]
async fn identifiers_are_version_7_uuids() {
    let t = two_tenants().await;
    let tx = tenant_tx(&api_pool(), t.a).await.unwrap();
    let id: Uuid = tx
        .client()
        .query_one(
            "insert into accounts (tenant_id, issuer_id, subject) values ($1, $2, 'v7') returning account_id",
            &[&t.a, &t.issuer],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(id.get_version_num(), 7);
    assert_eq!(t.a.get_version_num(), 7);
}

#[tokio::test]
async fn the_api_cannot_change_the_status_of_an_account() {
    let t = two_tenants().await;
    let tx = tenant_tx(&api_pool(), t.a).await.unwrap();
    let n = tx
        .client()
        .execute("update accounts set status = 'not-active'", &[])
        .await;
    assert_eq!(sqlstate(n), "42501");
}
