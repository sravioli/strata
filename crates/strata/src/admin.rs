//! `strata admin`: the operator command of tech designs 0003 and 0021, with the commands of
//! "Operator commands" in 0049. It connects directly as `strata_operator`, not through the pooler.

use clap::{Args, Subcommand, ValueEnum};
use tokio_postgres::Client;

use crate::auth::claims::MAX_CLAIM_BYTES;
use crate::auth::keys::jwks_uri;

#[derive(Subcommand, Debug)]
pub enum Admin {
    #[command(subcommand)]
    Tenant(TenantCmd),
    #[command(subcommand)]
    Issuer(IssuerCmd),
    #[command(subcommand)]
    Binding(BindingCmd),
    #[command(subcommand)]
    Account(AccountCmd),
}

#[derive(Subcommand, Debug)]
pub enum TenantCmd {
    Create { name: String },
    SetStatus { name: String, status: Status },
    List,
}

#[derive(Subcommand, Debug)]
pub enum IssuerCmd {
    Register(IssuerArgs),
    Set {
        issuer: String,
        #[command(flatten)]
        options: IssuerOptions,
        #[arg(long)]
        status: Option<Status>,
    },
}

#[derive(Args, Debug)]
pub struct IssuerArgs {
    #[arg(long)]
    pub issuer: String,
    #[arg(long)]
    pub jwks_url: String,
    #[arg(long)]
    pub audience: String,
    #[arg(long)]
    pub algorithm: Alg,
    #[arg(long)]
    pub tenant_claim: String,
    #[arg(long, default_value = "sub")]
    pub account_claim: String,
    #[arg(long, default_value_t = 3600, value_parser = clap::value_parser!(i32).range(1..=3600))]
    pub max_lifetime: i32,
}

#[derive(Args, Debug)]
pub struct IssuerOptions {
    #[arg(long)]
    pub jwks_url: Option<String>,
    #[arg(long)]
    pub audience: Option<String>,
    #[arg(long)]
    pub algorithm: Option<Alg>,
    #[arg(long)]
    pub tenant_claim: Option<String>,
    #[arg(long)]
    pub account_claim: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(i32).range(1..=3600))]
    pub max_lifetime: Option<i32>,
}

#[derive(Subcommand, Debug)]
pub enum BindingCmd {
    Add {
        #[arg(long)]
        issuer: String,
        #[arg(long)]
        tenant_value: String,
        #[arg(long)]
        tenant: String,
    },
    Remove {
        #[arg(long)]
        issuer: String,
        #[arg(long)]
        tenant_value: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum AccountCmd {
    SetStatus {
        #[arg(long)]
        tenant: String,
        #[arg(long)]
        issuer: String,
        #[arg(long)]
        subject: String,
        status: Status,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Active,
    NotActive,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Active => "active",
            Status::NotActive => "not-active",
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alg {
    #[value(name = "RS256")]
    Rs256,
    #[value(name = "ES256")]
    Es256,
    #[value(name = "EdDSA")]
    EdDsa,
}

impl Alg {
    fn as_str(self) -> &'static str {
        match self {
            Alg::Rs256 => "RS256",
            Alg::Es256 => "ES256",
            Alg::EdDsa => "EdDSA",
        }
    }
}

/// A tenant name: 1 to 63 characters, `a` to `z`, digits and `-`, with a letter first (0049).
pub fn valid_tenant_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && name.len() <= 63
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A tenant value or a subject: 1 to 256 bytes, as the claims of 0021.
pub fn valid_claim_value(v: &str) -> bool {
    !v.is_empty() && v.len() <= MAX_CLAIM_BYTES
}

fn check(ok: bool, what: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("invalid {what}"))
    }
}

fn one(rows: u64, what: &str) -> Result<(), String> {
    if rows == 1 {
        Ok(())
    } else {
        Err(format!("no such {what}"))
    }
}

/// Runs one operator command with a connection of `strata_operator`.
pub async fn run(c: &Client, cmd: Admin) -> Result<(), String> {
    let db = |e: tokio_postgres::Error| e.to_string();
    match cmd {
        Admin::Tenant(TenantCmd::Create { name }) => {
            check(valid_tenant_name(&name), "tenant name")?;
            let row = c
                .query_one(
                    "insert into tenants (name) values ($1) returning tenant_id::text",
                    &[&name],
                )
                .await
                .map_err(db)?;
            println!("{}", row.get::<_, String>(0));
        }
        Admin::Tenant(TenantCmd::SetStatus { name, status }) => {
            let n = c
                .execute(
                    "update tenants set status = $2 where name = $1",
                    &[&name, &status.as_str()],
                )
                .await
                .map_err(db)?;
            one(n, "tenant")?;
        }
        Admin::Tenant(TenantCmd::List) => {
            let rows = c
                .query(
                    "select tenant_id::text, name, status from tenants order by name",
                    &[],
                )
                .await
                .map_err(db)?;
            for r in rows {
                println!(
                    "{}\t{}\t{}",
                    r.get::<_, String>(1),
                    r.get::<_, String>(2),
                    r.get::<_, String>(0)
                );
            }
        }
        Admin::Issuer(IssuerCmd::Register(a)) => {
            check(
                jwks_uri(&a.jwks_url).is_some(),
                "JWKS URL: use HTTPS, or HTTP to a loopback address",
            )?;
            c.execute(
                "insert into issuers (issuer, jwks_url, audience, algorithm, tenant_claim, account_claim, \
                 max_lifetime_s) values ($1, $2, $3, $4, $5, $6, $7)",
                &[&a.issuer, &a.jwks_url, &a.audience, &a.algorithm.as_str(), &a.tenant_claim, &a.account_claim,
                  &a.max_lifetime],
            )
            .await
            .map_err(db)?;
        }
        Admin::Issuer(IssuerCmd::Set {
            issuer,
            options: o,
            status,
        }) => {
            if let Some(url) = &o.jwks_url {
                check(
                    jwks_uri(url).is_some(),
                    "JWKS URL: use HTTPS, or HTTP to a loopback address",
                )?;
            }
            let n = c
                .execute(
                    "update issuers set jwks_url = coalesce($2, jwks_url), audience = coalesce($3, audience), \
                     algorithm = coalesce($4, algorithm), tenant_claim = coalesce($5, tenant_claim), \
                     account_claim = coalesce($6, account_claim), \
                     max_lifetime_s = coalesce($7, max_lifetime_s), status = coalesce($8, status) \
                     where issuer = $1",
                    &[&issuer, &o.jwks_url, &o.audience, &o.algorithm.map(Alg::as_str), &o.tenant_claim,
                      &o.account_claim, &o.max_lifetime, &status.map(Status::as_str)],
                )
                .await
                .map_err(db)?;
            one(n, "issuer")?;
        }
        Admin::Binding(BindingCmd::Add {
            issuer,
            tenant_value,
            tenant,
        }) => {
            check(valid_claim_value(&tenant_value), "tenant value")?;
            let n = c
                .execute(
                    "insert into issuer_bindings (issuer_id, tenant_value, tenant_id) \
                     select i.issuer_id, $2, t.tenant_id from issuers i, tenants t \
                     where i.issuer = $1 and t.name = $3",
                    &[&issuer, &tenant_value, &tenant],
                )
                .await
                .map_err(db)?;
            one(n, "issuer or tenant")?;
        }
        Admin::Binding(BindingCmd::Remove {
            issuer,
            tenant_value,
        }) => {
            let n = c
                .execute(
                    "delete from issuer_bindings b using issuers i \
                     where b.issuer_id = i.issuer_id and i.issuer = $1 and b.tenant_value = $2",
                    &[&issuer, &tenant_value],
                )
                .await
                .map_err(db)?;
            one(n, "binding")?;
        }
        Admin::Account(AccountCmd::SetStatus {
            tenant,
            issuer,
            subject,
            status,
        }) => {
            let n = c
                .execute(
                    "update accounts a set status = $4 from tenants t, issuers i \
                     where a.tenant_id = t.tenant_id and a.issuer_id = i.issuer_id \
                     and t.name = $1 and i.issuer = $2 and a.subject = $3",
                    &[&tenant, &issuer, &subject, &status.as_str()],
                )
                .await
                .map_err(db)?;
            one(n, "account")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        admin: Admin,
    }

    fn parse(args: &[&str]) -> Result<Admin, clap::Error> {
        Cli::try_parse_from(std::iter::once("admin").chain(args.iter().copied())).map(|c| c.admin)
    }

    #[test]
    fn tenant_names() {
        for ok in ["acme", "a", "acme-2", &format!("a{}", "b".repeat(62))] {
            assert!(valid_tenant_name(ok), "{ok}");
        }
        for bad in [
            "",
            "Acme",
            "1acme",
            "-acme",
            "acme_corp",
            "acmé",
            &"a".repeat(64),
        ] {
            assert!(!valid_tenant_name(bad), "{bad}");
        }
    }

    #[test]
    fn claim_values() {
        assert!(valid_claim_value("acme-corp"));
        assert!(valid_claim_value(&"x".repeat(256)));
        assert!(!valid_claim_value(""));
        assert!(!valid_claim_value(&"x".repeat(257)));
    }

    #[test]
    fn issuer_register_defaults_and_ranges() {
        let base = [
            "issuer",
            "register",
            "--issuer",
            "p1",
            "--jwks-url",
            "https://p1/jwks",
            "--audience",
            "strata",
            "--tenant-claim",
            "org",
        ];
        let with = |extra: &[&str]| parse(&[&base[..], extra].concat());
        let Admin::Issuer(IssuerCmd::Register(a)) = with(&["--algorithm", "ES256"]).unwrap() else {
            panic!()
        };
        assert_eq!(
            (a.account_claim.as_str(), a.max_lifetime, a.algorithm),
            ("sub", 3600, Alg::Es256)
        );
        assert!(with(&["--algorithm", "EdDSA", "--max-lifetime", "1"]).is_ok());
        assert!(with(&["--algorithm", "RS256", "--max-lifetime", "0"]).is_err());
        assert!(with(&["--algorithm", "RS256", "--max-lifetime", "3601"]).is_err());
        assert!(with(&["--algorithm", "HS256"]).is_err());
    }

    #[test]
    fn statuses() {
        let Admin::Tenant(TenantCmd::SetStatus { status, .. }) =
            parse(&["tenant", "set-status", "acme", "not-active"]).unwrap()
        else {
            panic!()
        };
        assert_eq!(status.as_str(), "not-active");
        assert!(parse(&["tenant", "set-status", "acme", "off"]).is_err());
        assert_eq!(Status::Active.as_str(), "active");
        assert_eq!(Alg::EdDsa.as_str(), "EdDSA");
    }
}
