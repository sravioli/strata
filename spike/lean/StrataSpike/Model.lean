/-
Hand-written reference model of the lifecycle of one document revision.

This file uses only core Lean (no Mathlib, no Aeneas). Revision numbers are
unbounded natural numbers here; the Rust code uses `u32`.

The state is one revision (`number`, `status`) plus the number of the
document's active revision as this revision sees it (`active`).
An event that does not apply leaves the state unchanged.
-/

namespace Strata.Model

/-- Processing status of a revision. -/
inductive Status where
  | uploaded
  | processing
  | ready
  | failed
  | deleted
  deriving DecidableEq, Repr

/-- One revision and the document's active-revision pointer. -/
structure Revision where
  /-- Revision number. A larger number is a newer revision. -/
  number : Nat
  status : Status
  /-- Number of the document's active revision, if there is one. -/
  active : Option Nat
  deriving DecidableEq, Repr

inductive Event where
  /-- A worker took the revision: uploaded → processing. -/
  | startProcessing
  /-- Indexing finished: processing → ready. -/
  | processingSucceeded
  /-- Indexing failed: processing → failed. -/
  | processingFailed
  /-- Make this revision the active one. Applies only if the revision is
  ready and is newer than the current active revision. -/
  | activate
  /-- Another revision `n` of the same document became active. Applies only
  if `n` is not this revision and is newer than the current active revision. -/
  | otherActivated (n : Nat)
  /-- Delete the revision: any status → deleted. -/
  | delete
  deriving DecidableEq, Repr

/-- A new revision: uploaded, with the document's current active pointer. -/
def newRevision (number : Nat) (active : Option Nat) : Revision :=
  { number, status := .uploaded, active }

/-- `n` is newer than the active revision (or there is no active revision). -/
def newerThanActive : Option Nat → Nat → Bool
  | none, _ => true
  | some a, n => decide (a < n)

/-- Only a ready revision that is the active one is searchable. -/
def searchable (r : Revision) : Bool :=
  match r.status with
  | .ready =>
    match r.active with
    | none => false
    | some a => decide (a = r.number)
  | _ => false

/-- The transition function. -/
def step (r : Revision) : Event → Revision
  | .startProcessing =>
    match r.status with
    | .uploaded => { r with status := .processing }
    | _ => r
  | .processingSucceeded =>
    match r.status with
    | .processing => { r with status := .ready }
    | _ => r
  | .processingFailed =>
    match r.status with
    | .processing => { r with status := .failed }
    | _ => r
  | .activate =>
    match r.status with
    | .ready =>
      if newerThanActive r.active r.number then { r with active := some r.number } else r
    | _ => r
  | .otherActivated n =>
    if n = r.number then r
    else if newerThanActive r.active n then { r with active := some n } else r
  | .delete => { r with status := .deleted }

/-- Apply a list of events, first to last. -/
def run (r : Revision) (es : List Event) : Revision :=
  es.foldl step r

/-- Order on the active pointer: "no active revision" is below every number. -/
def ActiveLe : Option Nat → Option Nat → Prop
  | none, _ => True
  | some _, none => False
  | some a, some b => a ≤ b

@[simp] theorem activeLe_none (b : Option Nat) : ActiveLe none b := by
  simp [ActiveLe]

@[simp] theorem activeLe_some_none (a : Nat) : ¬ ActiveLe (some a) none := by
  simp [ActiveLe]

@[simp] theorem activeLe_some_some (a b : Nat) : ActiveLe (some a) (some b) ↔ a ≤ b := by
  simp [ActiveLe]

/-! ## Invariants -/

/-- Well-formed state: the active pointer points at this revision only if this
revision is ready or deleted. Every state that the system can reach is
well-formed (`newRevision_wf`, `step_wf`, `run_wf`). -/
def WF (r : Revision) : Prop :=
  r.active = some r.number → r.status = .ready ∨ r.status = .deleted

/-- Proof automation for the case analyses below: unfold the definitions,
split every `if`/`match`, then close the arithmetic goals. -/
local macro "crunch" : tactic =>
  `(tactic| (
    (try simp only [step, newerThanActive, searchable, WF]) <;>
    (repeat' split) <;>
    (try simp_all [newerThanActive]) <;>
    (repeat' split) <;>
    (try simp_all) <;>
    (try omega)))

/-- Invariant 1: a revision is searchable exactly when it is ready and it is the
active revision. -/
theorem searchable_iff (r : Revision) :
    searchable r = true ↔ r.status = .ready ∧ r.active = some r.number := by
  obtain ⟨number, status, active⟩ := r
  cases status <;> cases active <;> simp [searchable, eq_comm]

/-- A new revision whose number is newer than the active one is well-formed. -/
theorem newRevision_wf (number : Nat) (active : Option Nat)
    (h : newerThanActive active number = true) : WF (newRevision number active) := by
  cases active <;> simp_all [WF, newRevision, newerThanActive]
  omega

/-- A step keeps a well-formed state well-formed. -/
theorem step_wf (r : Revision) (e : Event) : WF r → WF (step r e) := by
  obtain ⟨number, status, active⟩ := r
  cases e <;> cases status <;> cases active <;> crunch

/-- Any number of steps keeps a well-formed state well-formed. -/
theorem run_wf (r : Revision) (es : List Event) (h : WF r) : WF (run r es) := by
  induction es generalizing r with
  | nil => exact h
  | cons e es ih => exact ih (step r e) (step_wf r e h)

/-- Invariant 1 (dynamic part): a well-formed revision that is not searchable
can become searchable in one step only if it is ready and the event is
`activate`. -/
theorem becomes_searchable_only_by_activate (r : Revision) (e : Event) :
    WF r → searchable r = false → searchable (step r e) = true →
    r.status = .ready ∧ e = .activate := by
  obtain ⟨number, status, active⟩ := r
  cases e <;> cases status <;> cases active <;> crunch

/-- A step never changes the revision number. -/
theorem step_number (r : Revision) (e : Event) : (step r e).number = r.number := by
  obtain ⟨number, status, active⟩ := r
  cases e <;> cases status <;> cases active <;> crunch

theorem ActiveLe.refl (a : Option Nat) : ActiveLe a a := by
  cases a <;> simp [ActiveLe]

theorem ActiveLe.trans {a b c : Option Nat} (h1 : ActiveLe a b) (h2 : ActiveLe b c) :
    ActiveLe a c := by
  cases a <;> cases b <;> cases c <;> simp_all [ActiveLe] <;> omega

/-- Invariant 2 (one step): a step never moves the active pointer to an older
revision and never clears it. -/
theorem step_active_mono (r : Revision) (e : Event) :
    ActiveLe r.active (step r e).active := by
  obtain ⟨number, status, active⟩ := r
  cases e <;> cases status <;> cases active <;> crunch

/-- Invariant 2 (any number of steps). -/
theorem run_active_mono (r : Revision) (es : List Event) :
    ActiveLe r.active (run r es).active := by
  induction es generalizing r with
  | nil => exact ActiveLe.refl _
  | cons e es ih => exact ActiveLe.trans (step_active_mono r e) (ih (step r e))

/-- Invariant 2, in plain form: if revision `a` is active and, after any
sequence of events, revision `b` is active, then `b` is not older than `a`. -/
theorem older_never_replaces_newer (r : Revision) (es : List Event) (a b : Nat)
    (ha : r.active = some a) (hb : (run r es).active = some b) : a ≤ b := by
  have h := run_active_mono r es
  rw [ha, hb] at h
  exact h

/-- Invariant 3 (one step): a deleted revision stays deleted. -/
theorem step_deleted (r : Revision) (e : Event) :
    r.status = .deleted → (step r e).status = .deleted := by
  obtain ⟨number, status, active⟩ := r
  cases e <;> cases status <;> cases active <;> crunch

/-- Invariant 3 (any number of steps): a deleted revision stays deleted. -/
theorem run_deleted (r : Revision) (es : List Event) (h : r.status = .deleted) :
    (run r es).status = .deleted := by
  induction es generalizing r with
  | nil => exact h
  | cons e es ih => exact ih (step r e) (step_deleted r e h)

/-- Invariant 3, in plain form: after a revision is deleted, no sequence of
events makes it searchable. -/
theorem deleted_never_searchable (r : Revision) (es : List Event)
    (h : r.status = .deleted) : searchable (run r es) = false := by
  have h' := run_deleted r es h
  simp [searchable, h']

/-- Invariant 3, starting from the delete event itself. -/
theorem after_delete_never_searchable (r : Revision) (es : List Event) :
    searchable (run r (.delete :: es)) = false :=
  deleted_never_searchable (step r .delete) es rfl

/-- Invariant 4: applying the same event twice gives the same state as
applying it once. -/
theorem step_idempotent (r : Revision) (e : Event) :
    step (step r e) e = step r e := by
  obtain ⟨number, status, active⟩ := r
  cases e with
  | otherActivated n =>
    cases active with
    | none => by_cases h : n = number <;> simp [step, newerThanActive, h]
    | some a =>
      by_cases h : n = number <;> by_cases h2 : a < n <;>
        simp [step, newerThanActive, h, h2]
  | activate =>
    cases status <;> cases active <;> simp [step, newerThanActive]
    rename_i a
    by_cases h : a < number <;> simp [h]
  | _ => cases status <;> simp [step]

end Strata.Model
