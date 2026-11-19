/-
The theorems of the "Proof obligations" table of tech design 0005, on the Rust function `job_step`
(Lean code that Aeneas generated in `Strata/Generated/`). Each theorem has its table name in the
namespace `Strata`, and `max` is `job.MAX_ATTEMPTS`. `job_step_refines` shows that `job_step` never
panics and agrees with the model `Strata/Model/Job.lean`.
-/
import Aeneas
import Strata.Generated.StrataCore
import Strata.Model.Job

open Aeneas Aeneas.Std Result
open strata_core

namespace Strata

/-! ## Abstraction: Rust values to model values -/

def absState : job.JobState → Model.JobState
  | .Waiting => .waiting | .Running => .running | .Parked => .parked | .Done => .done
def absError : job.ErrorClass → Model.ErrorClass
  | .NoError => .noError | .TenantNotActive => .tenantNotActive | .LeaseExpired => .leaseExpired | .JobFailed => .jobFailed
def absJobEvent : job.Event → Model.Event
  | .Claim b => .claim b | .Succeed => .succeed | .Fail => .fail | .LeaseExpired => .leaseExpired | .Requeue => .requeue
def absJob (j : job.Job) : Model.Job :=
  { state := absState j.state, attempts := j.attempts.val, runAfter := j.run_after.val,
    leaseUntil := j.lease_until.val, endedAt := j.ended_at.val, lastError := absError j.last_error }

theorem sat_val (x y : U64) :
    (core.num.U64.saturating_add x y).val = Model.satAdd x.val y.val := by
  have hx := x.hBounds
  have hy := y.hBounds
  unfold core.num.U64.saturating_add UScalar.saturating_add Model.satAdd Model.u64Max
  rw [UScalar.max_UScalarTy_U64_eq, U64.max_eq]
  show (BitVec.ofNat 64 (min 18446744073709551615 (x.val + y.val))).toNat = _
  rw [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega)]
  omega

@[step]
theorem backoff_refines (n : U32) : job.backoff_seconds n ⦃ b => b.val = Model.backoff n.val ⦄ := by
  unfold job.backoff_seconds
  split
  · simp; rfl
  · simp; rfl
  · simp; rfl
  · rename_i h1 h2 h3
    have : n.val ≠ 1 := fun h => h1 (UScalar.eq_equiv n 1#u32 |>.mpr (by simpa using h))
    have : n.val ≠ 2 := fun h => h2 (UScalar.eq_equiv n 2#u32 |>.mpr (by simpa using h))
    have : n.val ≠ 3 := fun h => h3 (UScalar.eq_equiv n 3#u32 |>.mpr (by simpa using h))
    simp
    unfold Model.backoff; split <;> simp_all

theorem job_step_refines (j : job.Job) (e : job.Event) (now : U64) :
    job.job_step j e now ⦃ j' => absJob j' = Model.jobStep (absJob j) (absJobEvent e) now.val ⦄ := by
  obtain ⟨st, att, ra, lu, ea, le⟩ := j
  unfold job.job_step
  by_cases ha : att.val < 5 <;>
    cases st <;> cases e <;> simp only [] <;> step* <;>
    simp_all [absJob, absJobEvent, absState, absError, Model.jobStep, sat_val, lift, job.MAX_ATTEMPTS,
      job.LEASE_SECONDS, Model.maxAttempts, Model.leaseSeconds] <;>
    (try split_ifs) <;> (try simp_all) <;>
    first | scalar_tac | simp [show ¬ ((↑att : Nat) < 5) by scalar_tac]

/-! ## The theorems of the 0005 table, on the Rust code -/

theorem max_attempts_val : (job.MAX_ATTEMPTS).val = Model.maxAttempts := by
  simp [job.MAX_ATTEMPTS, Model.maxAttempts]

theorem absState_eq (s : job.JobState) (m : Model.JobState) :
    absState s = m ↔ s = (match m with
      | .waiting => .Waiting | .running => .Running | .parked => .Parked | .done => .Done) := by
  cases s <;> cases m <;> simp [absState]

/-- A property of the model result holds for the Rust result. -/
theorem of_model {j : job.Job} {e : job.Event} {now : U64} {P : job.Job → Prop}
    (h : ∀ j', absJob j' = Model.jobStep (absJob j) (absJobEvent e) now.val → P j') :
    job.job_step j e now ⦃ j' => P j' ⦄ :=
  WP.spec_mono (job_step_refines j e now) h

theorem inactive_never_runs (j : job.Job) (now : U64) :
    j.state = .Waiting → job.job_step j (.Claim false) now ⦃ j' => j'.state ≠ .Running ⦄ := by
  intro hs
  refine of_model fun j' h => ?_
  have hm := Model.inactive_never_runs (absJob j) now.val (by simp [absJob, absState, hs])
  simp only [absJobEvent] at h
  rw [← h] at hm
  intro hr
  simp [absJob, hr, absState] at hm

theorem inactive_parks (j : job.Job) (now : U64) :
    j.state = .Waiting → job.job_step j (.Claim false) now ⦃ j' =>
      j' = { j with state := .Parked, last_error := .TenantNotActive } ⦄ := by
  intro hs
  unfold job.job_step
  simp [hs]

theorem attempts_bounded (j : job.Job) (e : job.Event) (now : U64) :
    j.attempts.val ≤ (job.MAX_ATTEMPTS).val →
      job.job_step j e now ⦃ j' => j'.attempts.val ≤ (job.MAX_ATTEMPTS).val ⦄ := by
  rw [max_attempts_val]
  intro ha
  refine of_model fun j' h => ?_
  have := Model.attempts_bounded (absJob j) (absJobEvent e) now.val (by simpa [absJob] using ha)
  rw [← h] at this
  simpa [absJob] using this

theorem fail_at_max_parks (j : job.Job) (e : job.Event) (now : U64) :
    j.state = .Running → j.attempts = job.MAX_ATTEMPTS → e ∈ [job.Event.Fail, job.Event.LeaseExpired] →
      job.job_step j e now ⦃ j' => j'.state = .Parked ⦄ := by
  intro hs ha he
  refine of_model fun j' h => ?_
  have hm := Model.fail_at_max_parks (absJob j) (absJobEvent e) now.val
    (by simp [absJob, absState, hs]) (by simp [absJob, ha, max_attempts_val])
    (by simp at he; rcases he with rfl | rfl <;> simp [absJobEvent])
  rw [← h] at hm
  simpa [absJob, absState_eq] using hm

theorem done_final (j : job.Job) (e : job.Event) (now : U64) :
    j.state = .Done → job.job_step j e now ⦃ j' => j' = j ⦄ := by
  intro hs
  unfold job.job_step
  cases e <;> simp [hs]

theorem parked_needs_operator (j : job.Job) (e : job.Event) (now : U64) :
    j.state = .Parked → e ≠ .Requeue → job.job_step j e now ⦃ j' => j'.state = .Parked ⦄ := by
  intro hs he
  refine of_model fun j' h => ?_
  have hm := Model.parked_needs_operator (absJob j) (absJobEvent e) now.val
    (by simp [absJob, absState, hs]) (by cases e <;> simp_all [absJobEvent])
  rw [← h] at hm
  simpa [absJob, absState_eq] using hm

end Strata
