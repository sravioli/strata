/-
Model of the job state function (tech design 0005, "Life of a job" and "The job state function").
The Lean statements of the "Proof obligations" table of 0005 are about these definitions, with
`max` = `maxAttempts`. `Strata/Proofs/Job.lean` restates them on the Rust code.
-/

namespace Strata.Model

inductive JobState where
  | waiting
  | running
  | parked
  | done
  deriving DecidableEq

/-- A class from a fixed list, never an error text (0005). -/
inductive ErrorClass where
  | noError
  | tenantNotActive
  | leaseExpired
  | jobFailed
  deriving DecidableEq

structure Job where
  state : JobState
  attempts : Nat
  runAfter : Nat
  leaseUntil : Nat
  endedAt : Nat
  lastError : ErrorClass

inductive Event where
  | claim (tenantActive : Bool)
  | succeed
  | fail
  | leaseExpired
  | requeue
  deriving DecidableEq

/-- 5 runs (owner decision of 2026-11-18 in 0005). -/
def maxAttempts : Nat := 5
def leaseSeconds : Nat := 300
def u64Max : Nat := 18446744073709551615

/-- The Rust code adds times with `saturating_add` on `u64`. -/
def satAdd (a b : Nat) : Nat := min (a + b) u64Max

/-- The backoff after the failed run number `n`: 30 s, 2 min, 10 min, 30 min. -/
def backoff : Nat → Nat
  | 1 => 30
  | 2 => 120
  | 3 => 600
  | _ => 1800

def jobStep (j : Job) (e : Event) (now : Nat) : Job :=
  match j.state, e with
  | .waiting, .claim false => { j with state := .parked, lastError := .tenantNotActive }
  | .waiting, .claim true =>
    -- A waiting job at the maximum is not run again (0005 has no such state; this keeps the bound).
    if j.attempts < maxAttempts then
      { j with state := .running, attempts := j.attempts + 1, leaseUntil := satAdd now leaseSeconds }
    else { j with state := .parked, lastError := .jobFailed }
  | .running, .succeed => { j with state := .done, endedAt := now }
  | .running, .fail =>
    if j.attempts < maxAttempts then
      { j with state := .waiting, runAfter := satAdd now (backoff j.attempts), lastError := .jobFailed }
    else { j with state := .parked, lastError := .jobFailed }
  | .running, .leaseExpired =>
    if j.attempts < maxAttempts then
      { j with state := .waiting, runAfter := now, lastError := .leaseExpired }
    else { j with state := .parked, lastError := .leaseExpired }
  | .parked, .requeue => { j with state := .waiting, attempts := 0, runAfter := now, lastError := .noError }
  | _, _ => j

/-! ## The statements of the 0005 table, on the model -/

theorem inactive_never_runs (j : Job) (now : Nat) :
    j.state = .waiting → (jobStep j (.claim false) now).state ≠ .running := by
  intro h; simp [jobStep, h]

theorem inactive_parks (j : Job) (now : Nat) :
    j.state = .waiting →
      jobStep j (.claim false) now = { j with state := .parked, lastError := .tenantNotActive } := by
  intro h; simp [jobStep, h]

theorem attempts_bounded (j : Job) (e : Event) (now : Nat) :
    j.attempts ≤ maxAttempts → (jobStep j e now).attempts ≤ maxAttempts := by
  intro h
  unfold jobStep
  split <;> (try split) <;> simp_all <;> omega

theorem fail_at_max_parks (j : Job) (e : Event) (now : Nat) :
    j.state = .running → j.attempts = maxAttempts → e ∈ [Event.fail, Event.leaseExpired] →
      (jobStep j e now).state = .parked := by
  intro hs ha he
  simp at he
  rcases he with rfl | rfl <;> simp [jobStep, hs, ha]

theorem done_final (j : Job) (e : Event) (now : Nat) : j.state = .done → jobStep j e now = j := by
  intro h
  unfold jobStep
  split <;> simp_all

theorem parked_needs_operator (j : Job) (e : Event) (now : Nat) :
    j.state = .parked → e ≠ .requeue → (jobStep j e now).state = .parked := by
  intro h he
  unfold jobStep
  split <;> simp_all

end Strata.Model
