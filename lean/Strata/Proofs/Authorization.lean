/-
The theorems of the "Proof obligations" table of tech design 0004, on the Rust functions
`authorize` and `authorized_scope` (Lean code that Aeneas generated in `Strata/Generated/`).
Each theorem has its table name in the namespace `Strata`. The refinement theorems show that
the Rust functions never panic and agree with the model `Strata/Model/Authorization.lean`.
-/
import Aeneas
import Strata.Generated.StrataCore
import Strata.Model.Authorization

open Aeneas Aeneas.Std Result
open strata_core

namespace Strata

/-! ## Abstraction: Rust values to model values -/

def absPrincipal : authorization.Principal → Model.Principal
  | .Account t c aa ta => .account t.val c.val aa ta
  | .Job t ta => .job t.val ta

def absAction : authorization.Action → Model.Action
  | .Search => .search
  | .ReadDocument => .readDocument
  | .UploadDocument => .uploadDocument
  | .DeleteDocument => .deleteDocument
  | .ProcessDocument => .processDocument

def absResource (r : authorization.Resource) : Model.Resource := ⟨r.tenant.val⟩

def absScope : authorization.Scope → Model.Scope
  | .Nothing => .nothing
  | .Tenant t => .tenant t.val

/-- "Allow" of the Rust decision. -/
def isAllow : authorization.Decision → Bool
  | .Allow => true
  | .Deny _ => false

/-- The tenant of a Rust principal, as a number. -/
def principalTenant (p : authorization.Principal) : Nat := (absPrincipal p).tenant

/-! ## Refinement -/

theorem authorize_refines (p : authorization.Principal) (a : authorization.Action)
    (r : authorization.Resource) :
    ∃ d, authorization.authorize p a r = ok d ∧
      isAllow d = Model.authorize (absPrincipal p) (absAction a) (absResource r) := by
  rcases p with ⟨t, c, aa, ta⟩ | ⟨t, ta⟩ <;> cases a <;> (try cases aa) <;> cases ta <;>
    by_cases h : r.tenant.val = t.val <;>
    simp_all [authorization.authorize, authorization.is_active, authorization.action_allowed,
      authorization.principal_tenant, Model.authorize, Model.permitted, Model.Action.forAccount,
      Model.Principal.tenant, absPrincipal, absAction, absResource, isAllow]

theorem authorized_scope_refines (p : authorization.Principal) (a : authorization.Action) :
    ∃ s, authorization.authorized_scope p a = ok s ∧
      absScope s = Model.authorizedScope (absPrincipal p) (absAction a) := by
  rcases p with ⟨t, c, aa, ta⟩ | ⟨t, ta⟩ <;> cases a <;> (try cases aa) <;> cases ta <;>
    simp_all [authorization.authorized_scope, authorization.is_active, authorization.action_allowed,
      authorization.principal_tenant, Model.authorizedScope, Model.permitted,
      Model.Action.forAccount, Model.Principal.tenant, absPrincipal, absAction, absScope]

/-- `authorize_total` (0004): the two functions give a result for each input and never stop
with an error, and the result is the result of the model. -/
theorem authorize_total :
    (∀ p a r, ∃ d, authorization.authorize p a r = ok d ∧
      isAllow d = Model.authorize (absPrincipal p) (absAction a) (absResource r)) ∧
    (∀ p a, ∃ s, authorization.authorized_scope p a = ok s ∧
      absScope s = Model.authorizedScope (absPrincipal p) (absAction a)) :=
  ⟨authorize_refines, authorized_scope_refines⟩

/-- A Rust decision is "allow" exactly when the model gives "allow". -/
theorem rust_allow (p : authorization.Principal) (a : authorization.Action)
    (r : authorization.Resource) :
    authorization.authorize p a r = ok .Allow ↔
      Model.authorize (absPrincipal p) (absAction a) (absResource r) = true := by
  obtain ⟨d, hd, hm⟩ := authorize_refines p a r
  rw [hd, ← hm]
  cases d <;> simp [isAllow]

/-- The Rust scope contains a resource exactly when the model scope contains it. -/
theorem rust_scope (p : authorization.Principal) (a : authorization.Action) :
    ∃ s, authorization.authorized_scope p a = ok s ∧
      ∀ r, (absScope s).contains (absResource r) =
        (Model.authorizedScope (absPrincipal p) (absAction a)).contains (absResource r) := by
  obtain ⟨s, hs, hm⟩ := authorized_scope_refines p a
  exact ⟨s, hs, fun r => by rw [hm]⟩

/-! ## The theorems of the 0004 table, on the Rust code -/

theorem allow_same_tenant (p : authorization.Principal) (a : authorization.Action)
    (r : authorization.Resource) :
    authorization.authorize p a r = ok .Allow → r.tenant.val = principalTenant p := by
  rw [rust_allow]
  exact Model.allow_same_tenant _ _ _

theorem allow_account_active (t c : U64) (aa ta : Bool) (a : authorization.Action)
    (r : authorization.Resource) :
    authorization.authorize (.Account t c aa ta) a r = ok .Allow → aa = true ∧ ta = true := by
  rw [rust_allow]
  exact Model.allow_account_active _ _ _ _ _ _

theorem active_account_allowed (t c : U64) (a : authorization.Action)
    (r : authorization.Resource) :
    (absAction a).forAccount = true → r.tenant = t →
      authorization.authorize (.Account t c true true) a r = ok .Allow := by
  intro ha hr
  rw [rust_allow]
  exact Model.active_account_allowed _ _ _ _ ha (by simp [absResource, hr])

theorem account_never_processes (t c : U64) (aa ta : Bool) (r : authorization.Resource) :
    ∃ reason, authorization.authorize (.Account t c aa ta) .ProcessDocument r = ok (.Deny reason) := by
  obtain ⟨d, hd, hm⟩ := authorize_refines (.Account t c aa ta) .ProcessDocument r
  cases d with
  | Allow => simp [isAllow, absPrincipal, absAction, Model.account_never_processes] at hm
  | Deny reason => exact ⟨reason, hd⟩

theorem job_only_processes (t : U64) (ta : Bool) (a : authorization.Action)
    (r : authorization.Resource) :
    authorization.authorize (.Job t ta) a r = ok .Allow →
      a = .ProcessDocument ∧ r.tenant = t := by
  rw [rust_allow]
  intro h
  obtain ⟨ha, hr⟩ := Model.job_only_processes _ _ _ _ h
  refine ⟨by cases a <;> simp_all [absAction], ?_⟩
  exact UScalar.eq_equiv _ _ |>.mpr (by simpa [absResource] using hr)

theorem allow_job_active (t : U64) (ta : Bool) (a : authorization.Action)
    (r : authorization.Resource) :
    authorization.authorize (.Job t ta) a r = ok .Allow → ta = true := by
  rw [rust_allow]
  exact Model.allow_job_active _ _ _ _

theorem scope_agrees (p : authorization.Principal) (a : authorization.Action)
    (r : authorization.Resource) :
    ∃ s d, authorization.authorized_scope p a = ok s ∧ authorization.authorize p a r = ok d ∧
      (absScope s).contains (absResource r) = isAllow d := by
  obtain ⟨s, hs, hm⟩ := authorized_scope_refines p a
  obtain ⟨d, hd, hdm⟩ := authorize_refines p a r
  exact ⟨s, d, hs, hd, by rw [hm, hdm, Model.scope_agrees]⟩

theorem scope_one_tenant (p : authorization.Principal) (a : authorization.Action) :
    ∃ s, authorization.authorized_scope p a = ok s ∧
      ∀ r, (absScope s).contains (absResource r) = true → r.tenant.val = principalTenant p := by
  obtain ⟨s, hs, hm⟩ := authorized_scope_refines p a
  exact ⟨s, hs, fun r h => Model.scope_one_tenant _ _ _ (by rw [← hm]; exact h)⟩

end Strata
