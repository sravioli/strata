/-
Proof that the Lean code Aeneas generated from the Rust crate `domain-core`
(`StrataSpike/Generated/DomainCore.lean`) agrees with the hand-written model
(`StrataSpike/Model.lean`), and the four invariants restated on the Rust code.
-/
import Aeneas
import StrataSpike.Generated.DomainCore
import StrataSpike.Model

open Aeneas Std Result

namespace Strata.Refinement

open Strata

/-! ## Abstraction: Rust values → model values -/

/-- Rust `Status` → model `Status`. -/
def absStatus : domain_core.Status → Model.Status
  | .Uploaded => .uploaded
  | .Processing => .processing
  | .Ready => .ready
  | .Failed => .failed
  | .Deleted => .deleted

/-- Rust `Option<u32>` → model `Option Nat`. -/
def absActive : Option U32 → Option Nat
  | none => none
  | some a => some a.val

@[simp] theorem absActive_none : absActive none = none := rfl

@[simp] theorem absActive_some (a : U32) : absActive (some a) = some a.val := rfl

/-- Rust `Revision` → model `Revision`. -/
def absRev (r : domain_core.Revision) : Model.Revision :=
  { number := r.number.val, status := absStatus r.status, active := absActive r.active }

/-- Rust `Event` → model `Event`. -/
def absEvent : domain_core.Event → Model.Event
  | .StartProcessing => .startProcessing
  | .ProcessingSucceeded => .processingSucceeded
  | .ProcessingFailed => .processingFailed
  | .Activate => .activate
  | .OtherActivated n => .otherActivated n.val
  | .Delete => .delete

theorem absStatus_inj {a b : domain_core.Status} (h : absStatus a = absStatus b) : a = b := by
  cases a <;> cases b <;> simp_all [absStatus]

theorem absActive_inj {a b : Option U32} (h : absActive a = absActive b) : a = b := by
  cases a <;> cases b <;> simp_all [absActive]
  exact UScalar.eq_equiv _ _ |>.mpr h

/-- Two different Rust states never map to the same model state. -/
theorem absRev_inj {a b : domain_core.Revision} (h : absRev a = absRev b) : a = b := by
  obtain ⟨n1, s1, a1⟩ := a
  obtain ⟨n2, s2, a2⟩ := b
  simp only [absRev, Model.Revision.mk.injEq] at h
  obtain ⟨hn, hs, ha⟩ := h
  have := UScalar.eq_equiv n1 n2 |>.mpr hn
  have := absStatus_inj hs
  have := absActive_inj ha
  simp_all

/-! ## Rust meets model -/

/-- The Rust `newer_than_active` never panics and returns the model's answer. -/
theorem newer_than_active_refines (active : Option U32) (n : U32) :
    domain_core.newer_than_active active n
      = ok (Model.newerThanActive (absActive active) n.val) := by
  cases active <;> simp [domain_core.newer_than_active, Model.newerThanActive, absActive]

/-- The Rust `new_revision` never panics and returns the model's new revision. -/
theorem new_revision_refines (number : U32) (active : Option U32) :
    ∃ r, domain_core.new_revision number active = ok r ∧
      absRev r = Model.newRevision number.val (absActive active) := by
  simp [domain_core.new_revision, Model.newRevision, absRev, absStatus]

/-- The Rust `searchable` never panics and returns the model's answer. -/
theorem searchable_refines (r : domain_core.Revision) :
    domain_core.searchable r = ok (Model.searchable (absRev r)) := by
  obtain ⟨number, status, active⟩ := r
  cases status <;> cases active <;>
    simp [domain_core.searchable, Model.searchable, absRev, absStatus, UScalar.eq_equiv]
  congr

/-- Main refinement theorem: the Rust `step` never panics, and its result maps
to the result of the model's `step` on the mapped inputs. -/
theorem step_refines (r : domain_core.Revision) (e : domain_core.Event) :
    ∃ r', domain_core.step r e = ok r' ∧
      absRev r' = Model.step (absRev r) (absEvent e) := by
  obtain ⟨number, status, active⟩ := r
  cases e <;> cases status <;>
    simp [domain_core.step, Model.step, absRev, absStatus, absEvent,
      newer_than_active_refines, UScalar.eq_equiv] <;>
    (repeat' split) <;> simp_all

/-- If the Rust `step` returns `r1`, then `r1` maps to the model's result. -/
theorem step_ok {r r1 : domain_core.Revision} {e : domain_core.Event}
    (h : domain_core.step r e = ok r1) :
    absRev r1 = Model.step (absRev r) (absEvent e) := by
  obtain ⟨r', h', habs⟩ := step_refines r e
  rw [h] at h'
  rw [Result.ok_injective h']
  exact habs

/-- `RunsTo r es r'`: the Rust `step`, applied to the events `es` first to
last and starting from `r`, returns `r'` and never fails on the way. -/
inductive RunsTo : domain_core.Revision → List domain_core.Event → domain_core.Revision → Prop
  | nil (r : domain_core.Revision) : RunsTo r [] r
  | cons {r r1 r2 : domain_core.Revision} {e : domain_core.Event} {es : List domain_core.Event} :
      domain_core.step r e = ok r1 → RunsTo r1 es r2 → RunsTo r (e :: es) r2

/-- A sequence of Rust steps never fails: it always returns a final state. -/
theorem runsTo_total (r : domain_core.Revision) (es : List domain_core.Event) :
    ∃ r', RunsTo r es r' := by
  induction es generalizing r with
  | nil => exact ⟨r, .nil r⟩
  | cons e es ih =>
    obtain ⟨r1, h1, _⟩ := step_refines r e
    obtain ⟨r2, h2⟩ := ih r1
    exact ⟨r2, .cons h1 h2⟩

/-- A sequence of Rust steps maps to the same sequence of model steps. -/
theorem run_refines {r r' : domain_core.Revision} {es : List domain_core.Event}
    (h : RunsTo r es r') :
    absRev r' = Model.run (absRev r) (es.map absEvent) := by
  induction h with
  | nil r => rfl
  | cons hstep _ ih =>
    rw [ih, step_ok hstep]
    rfl

/-! ## The four invariants, restated on the Rust code -/

/-- Invariant 1 on Rust: the Rust `searchable` returns true exactly when the
status is `Ready` and the active pointer is this revision's number. -/
theorem rust_searchable_iff (r : domain_core.Revision) :
    domain_core.searchable r = ok true ↔
      r.status = .Ready ∧ r.active = some r.number := by
  rw [searchable_refines, Result.ok.injEq, Model.searchable_iff]
  obtain ⟨number, status, active⟩ := r
  cases status <;> cases active <;> simp [absRev, absStatus, UScalar.eq_equiv]

/-- Invariant 1 on Rust (dynamic part): a well-formed Rust revision that is not
searchable can become searchable in one Rust step only if it is `Ready` and the
event is `Activate`. -/
theorem rust_becomes_searchable_only_by_activate
    (r r1 : domain_core.Revision) (e : domain_core.Event)
    (hwf : Model.WF (absRev r))
    (h0 : domain_core.searchable r = ok false)
    (hs : domain_core.step r e = ok r1)
    (h1 : domain_core.searchable r1 = ok true) :
    r.status = .Ready ∧ e = .Activate := by
  rw [searchable_refines, Result.ok.injEq] at h0 h1
  rw [step_ok hs] at h1
  obtain ⟨hst, hev⟩ :=
    Model.becomes_searchable_only_by_activate (absRev r) (absEvent e) hwf h0 h1
  constructor
  · apply absStatus_inj; simpa [absRev, absStatus] using hst
  · cases e <;> simp_all [absEvent]

/-- A new Rust revision whose number is newer than the active one is
well-formed. -/
theorem rust_new_revision_wf (number : U32) (active : Option U32) (r : domain_core.Revision)
    (hnew : domain_core.newer_than_active active number = ok true)
    (h : domain_core.new_revision number active = ok r) : Model.WF (absRev r) := by
  obtain ⟨r', h', habs⟩ := new_revision_refines number active
  rw [h] at h'
  rw [Result.ok_injective h', habs]
  rw [newer_than_active_refines, Result.ok.injEq] at hnew
  exact Model.newRevision_wf _ _ hnew

/-- Every Rust state reached by Rust steps from a well-formed state is
well-formed. -/
theorem rust_run_wf {r r' : domain_core.Revision} {es : List domain_core.Event}
    (hrun : RunsTo r es r') (h : Model.WF (absRev r)) : Model.WF (absRev r') := by
  rw [run_refines hrun]
  exact Model.run_wf _ _ h

/-- Invariant 2 on Rust: if revision `a` is active and, after any sequence of
Rust steps, revision `b` is active, then `b` is not older than `a`. -/
theorem rust_older_never_replaces_newer {r r' : domain_core.Revision}
    {es : List domain_core.Event} (hrun : RunsTo r es r') (a b : U32)
    (ha : r.active = some a) (hb : r'.active = some b) : a.val ≤ b.val := by
  apply Model.older_never_replaces_newer (absRev r) (es.map absEvent)
  · simp [absRev, ha]
  · rw [← run_refines hrun]; simp [absRev, hb]

/-- Invariant 3 on Rust: after the Rust `step` applies `Delete`, no sequence of
Rust steps makes the revision searchable. -/
theorem rust_after_delete_never_searchable {r r' : domain_core.Revision}
    {es : List domain_core.Event} (hrun : RunsTo r (.Delete :: es) r') :
    domain_core.searchable r' = ok false := by
  rw [searchable_refines, run_refines hrun]
  simp only [List.map_cons, absEvent]
  rw [Model.after_delete_never_searchable]

/-- Invariant 4 on Rust: the Rust `step` applied twice with the same event
returns the same state as applied once. -/
theorem rust_step_idempotent (r r1 : domain_core.Revision) (e : domain_core.Event)
    (h : domain_core.step r e = ok r1) : domain_core.step r1 e = ok r1 := by
  obtain ⟨r2, h2, habs⟩ := step_refines r1 e
  rw [h2]
  congr 1
  apply absRev_inj
  rw [habs, step_ok h, Model.step_idempotent]

end Strata.Refinement
