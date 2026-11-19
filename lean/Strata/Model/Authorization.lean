/-
Model of the v1 authorization rule (tech design 0004, "The decision function" and
"The authorized scope"). The Lean statements of the "Proof obligations" table of 0004 are
about these definitions. `Strata/Proofs/Authorization.lean` restates them on the Rust code.
-/

namespace Strata.Model

/-- An account carries its own status and the status of its tenant; a job carries the status
of its tenant. Identifiers are natural numbers. -/
inductive Principal where
  | account (tenant account : Nat) (accountActive tenantActive : Bool)
  | job (tenant : Nat) (tenantActive : Bool)

def Principal.tenant : Principal → Nat
  | .account t _ _ _ => t
  | .job t _ => t

inductive Action where
  | search
  | readDocument
  | uploadDocument
  | deleteDocument
  | processDocument
  deriving DecidableEq

/-- The actions that an account can take. -/
def Action.forAccount : Action → Bool
  | .processDocument => false
  | _ => true

structure Resource where
  tenant : Nat

inductive Scope where
  | nothing
  | tenant (t : Nat)

def Scope.contains : Scope → Resource → Bool
  | .nothing, _ => false
  | .tenant t, r => r.tenant == t

/-- Part 1 of the rule: the rule permits the action for the principal. -/
def permitted : Principal → Action → Bool
  | .account _ _ aa ta, a => aa && ta && a.forAccount
  | .job _ ta, a => ta && a == .processDocument

/-- Part 1 and part 2: the resource belongs to the tenant of the principal. -/
def authorize (p : Principal) (a : Action) (r : Resource) : Bool :=
  permitted p a && r.tenant == p.tenant

def authorizedScope (p : Principal) (a : Action) : Scope :=
  if permitted p a then .tenant p.tenant else .nothing

/-! ## The statements of the 0004 table, on the model -/

theorem allow_same_tenant (p : Principal) (a : Action) (r : Resource) :
    authorize p a r = true → r.tenant = p.tenant := by
  simp [authorize]

theorem allow_account_active (t c : Nat) (aa ta : Bool) (a : Action) (r : Resource) :
    authorize (.account t c aa ta) a r = true → aa = true ∧ ta = true := by
  cases aa <;> cases ta <;> simp [authorize, permitted]

theorem active_account_allowed (t c : Nat) (a : Action) (r : Resource) :
    a.forAccount = true → r.tenant = t → authorize (.account t c true true) a r = true := by
  intro ha hr
  simp [authorize, permitted, Principal.tenant, ha, hr]

theorem account_never_processes (t c : Nat) (aa ta : Bool) (r : Resource) :
    authorize (.account t c aa ta) .processDocument r = false := by
  simp [authorize, permitted, Action.forAccount]

theorem job_only_processes (t : Nat) (ta : Bool) (a : Action) (r : Resource) :
    authorize (.job t ta) a r = true → a = .processDocument ∧ r.tenant = t := by
  simp [authorize, permitted, Principal.tenant]
  intro _ ha hr
  exact ⟨ha, hr⟩

theorem allow_job_active (t : Nat) (ta : Bool) (a : Action) (r : Resource) :
    authorize (.job t ta) a r = true → ta = true := by
  cases ta <;> simp [authorize, permitted]

theorem scope_agrees (p : Principal) (a : Action) (r : Resource) :
    (authorizedScope p a).contains r = authorize p a r := by
  unfold authorizedScope authorize
  cases permitted p a <;> simp [Scope.contains]

theorem scope_one_tenant (p : Principal) (a : Action) (r : Resource) :
    (authorizedScope p a).contains r = true → r.tenant = p.tenant := by
  rw [scope_agrees]
  exact allow_same_tenant p a r

end Strata.Model
