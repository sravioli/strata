//! The v1 authorization rule (tech design 0004). The theorems of its "Proof obligations"
//! table are about `authorize` and `authorized_scope` (`lean/Strata/Proofs/Authorization.lean`).
//! Proved subset of tech design 0009: enums, structs, `match` and comparison only.

/// Who makes a request. A client cannot supply a principal: the API builds it from the
/// verified token, and a worker builds it from the job it claimed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Principal {
    Account {
        tenant: u64,
        account: u64,
        account_active: bool,
        tenant_active: bool,
    },
    Job {
        tenant: u64,
        tenant_active: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Search,
    ReadDocument,
    UploadDocument,
    DeleteDocument,
    ProcessDocument,
    RemoveDocumentFiles,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resource {
    pub tenant: u64,
}

/// The reason of a "deny". It goes to the log only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenyReason {
    NotActive,
    NotPermitted,
    OtherTenant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny(DenyReason),
}

/// The resources that a principal can reach. Only `authorized_scope` creates one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope(ScopeKind);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeKind {
    Nothing,
    Tenant(u64),
}

impl Scope {
    /// The tenant whose resources the scope holds, or `None` for the scope "Nothing".
    pub fn tenant(self) -> Option<u64> {
        match self.0 {
            ScopeKind::Nothing => None,
            ScopeKind::Tenant(t) => Some(t),
        }
    }
}

fn principal_tenant(p: Principal) -> u64 {
    match p {
        Principal::Account { tenant, .. } => tenant,
        Principal::Job { tenant, .. } => tenant,
    }
}

fn is_active(p: Principal) -> bool {
    match p {
        Principal::Account {
            account_active,
            tenant_active,
            ..
        } => account_active && tenant_active,
        Principal::Job { tenant_active, .. } => tenant_active,
    }
}

/// The job actions. An account gets only the other actions, and a job gets only these.
fn is_job_action(a: Action) -> bool {
    matches!(a, Action::ProcessDocument | Action::RemoveDocumentFiles)
}

fn action_allowed(p: Principal, a: Action) -> bool {
    match p {
        Principal::Account { .. } => !is_job_action(a),
        Principal::Job { .. } => is_job_action(a),
    }
}

/// "Allow" only when the principal is active, the rule permits the action, and the resource
/// belongs to the tenant of the principal.
pub fn authorize(p: Principal, a: Action, r: Resource) -> Decision {
    if !is_active(p) {
        Decision::Deny(DenyReason::NotActive)
    } else if !action_allowed(p, a) {
        Decision::Deny(DenyReason::NotPermitted)
    } else if r.tenant != principal_tenant(p) {
        Decision::Deny(DenyReason::OtherTenant)
    } else {
        Decision::Allow
    }
}

/// The scope "Tenant" of the principal when part 1 of the rule is true, else "Nothing".
pub fn authorized_scope(p: Principal, a: Action) -> Scope {
    if is_active(p) && action_allowed(p, a) {
        Scope(ScopeKind::Tenant(principal_tenant(p)))
    } else {
        Scope(ScopeKind::Nothing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANNA: Principal = Principal::Account {
        tenant: 1,
        account: 7,
        account_active: true,
        tenant_active: true,
    };

    #[test]
    fn deny_reasons() {
        let doc = Resource { tenant: 1 };
        let off = Principal::Account {
            tenant: 1,
            account: 7,
            account_active: false,
            tenant_active: true,
        };
        assert_eq!(
            authorize(off, Action::Search, doc),
            Decision::Deny(DenyReason::NotActive)
        );
        assert_eq!(
            authorize(ANNA, Action::ProcessDocument, doc),
            Decision::Deny(DenyReason::NotPermitted)
        );
        assert_eq!(
            authorize(ANNA, Action::Search, Resource { tenant: 2 }),
            Decision::Deny(DenyReason::OtherTenant)
        );
        assert_eq!(authorize(ANNA, Action::Search, doc), Decision::Allow);
    }

    #[test]
    fn job_actions() {
        let job = Principal::Job {
            tenant: 1,
            tenant_active: true,
        };
        let doc = Resource { tenant: 1 };
        assert_eq!(
            authorize(job, Action::RemoveDocumentFiles, doc),
            Decision::Allow
        );
        assert_eq!(
            authorize(job, Action::RemoveDocumentFiles, Resource { tenant: 2 }),
            Decision::Deny(DenyReason::OtherTenant)
        );
        assert_eq!(
            authorize(ANNA, Action::RemoveDocumentFiles, doc),
            Decision::Deny(DenyReason::NotPermitted)
        );
        assert_eq!(
            authorized_scope(job, Action::RemoveDocumentFiles).tenant(),
            Some(1)
        );
        assert_eq!(
            authorized_scope(ANNA, Action::RemoveDocumentFiles).tenant(),
            None
        );
    }

    #[test]
    fn scope_tenant() {
        assert_eq!(authorized_scope(ANNA, Action::Search).tenant(), Some(1));
        assert_eq!(
            authorized_scope(ANNA, Action::ProcessDocument).tenant(),
            None
        );
    }
}
