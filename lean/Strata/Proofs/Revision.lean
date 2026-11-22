/-
The theorems of the "Proof obligations" table of tech design 0049, on the Rust functions
`revision_step` and `may_extract` (Lean code that Aeneas generated in `Strata/Generated/`). Each
theorem has its table name in the namespace `Strata`. The Lean text differs from the table in one
place, with the same meaning: the Rust field `error_class` is an `Option`, so its value is `some c`.
`revision_step_refines` and `may_extract_refines` show that the functions never panic and agree
with the model `Strata/Model/Revision.lean`.
-/
import Aeneas
import Strata.Generated.StrataCore
import Strata.Model.Revision

open Aeneas Aeneas.Std Result
open strata_core

namespace Strata

/-! ## Abstraction: Rust values to model values -/

def absRevState : revision.RevisionState → Model.RevisionState
  | .AwaitingUpload => .awaitingUpload | .Uploaded => .uploaded | .Ready => .ready
  | .Failed => .failed | .Superseded => .superseded
def absClass : revision.FailureClass → Model.FailureClass
  | .FileTooLarge => .fileTooLarge | .ExtractionTimeout => .extractionTimeout
  | .ExtractionMemory => .extractionMemory | .ExtractionCrash => .extractionCrash
  | .ExtractionOutputTooLarge => .extractionOutputTooLarge | .ArchiveTooLarge => .archiveTooLarge
  | .UnsupportedFormat => .unsupportedFormat | .EncryptedFile => .encryptedFile
  | .MalformedFile => .malformedFile | .InvalidOutput => .invalidOutput
def absRevEvent : revision.Event → Model.RevisionEvent
  | .Finish => .finish | .FinishTooLarge => .finishTooLarge | .Extracted n => .extracted n
  | .ExtractFailed c => .extractFailed (absClass c) | .NewerReady => .newerReady
def absRev (r : revision.Revision) : Model.Revision :=
  { state := absRevState r.state, retryMark := r.retry_mark,
    errorClass := r.error_class.map absClass, noText := r.no_text }

/-- A class that retries one time (0007): a crash, the time limit or invalid output. -/
def retries (c : revision.FailureClass) : Prop :=
  c = .ExtractionCrash ∨ c = .ExtractionTimeout ∨ c = .InvalidOutput

/-! ## Refinement -/

theorem revision_step_refines (r : revision.Revision) (e : revision.Event) :
    ∃ r', revision.revision_step r e = ok r' ∧
      absRev r' = Model.revisionStep (absRev r) (absRevEvent e) := by
  obtain ⟨st, m, ec, nt⟩ := r
  cases st <;> cases e <;> (try rename_i c; cases c) <;> (try cases m) <;>
    simp [revision.revision_step, Model.revisionStep, Model.retries, absRev, absRevState,
      absRevEvent, absClass]

theorem may_extract_refines (r : revision.Revision) :
    ∃ b, revision.may_extract r = ok b ∧ b = Model.mayExtract (absRev r) := by
  obtain ⟨st, m, ec, nt⟩ := r
  cases st <;> simp [revision.may_extract, Model.mayExtract, absRev, absRevState]

/-! ## The theorems of the 0049 table, on the Rust code -/

/-- `revision_total` (0009): the two functions give a result for each input and never stop with
an error, and the result is the result of the model. -/
theorem revision_total :
    (∀ r e, ∃ r', revision.revision_step r e = ok r' ∧
      absRev r' = Model.revisionStep (absRev r) (absRevEvent e)) ∧
    (∀ r, ∃ b, revision.may_extract r = ok b ∧ b = Model.mayExtract (absRev r)) :=
  ⟨revision_step_refines, may_extract_refines⟩

theorem final_sticky (r : revision.Revision) (e : revision.Event) :
    (r.state = .Failed ∨ r.state = .Superseded) → revision.revision_step r e = ok r := by
  rintro (h | h) <;> cases e <;> simp [revision.revision_step, h]

theorem ready_only_superseded (r : revision.Revision) (e : revision.Event) :
    r.state = .Ready → ∃ r', revision.revision_step r e = ok r' ∧
      (r'.state = .Ready ∨ r'.state = .Superseded) := by
  intro h
  cases e <;> simp [revision.revision_step, h]

theorem retry_once (r : revision.Revision) (c : revision.FailureClass) :
    r.state = .Uploaded → retries c → ∃ r', revision.revision_step r (.ExtractFailed c) = ok r' ∧
      (r.retry_mark = false → r'.state = .Uploaded ∧ r'.retry_mark = true) ∧
      (r.retry_mark = true → r'.state = .Failed ∧ r'.error_class = some c) := by
  intro hs hc
  obtain ⟨st, m, ec, nt⟩ := r
  simp only at hs
  subst hs
  rcases hc with rfl | rfl | rfl <;> cases m <;> simp [revision.revision_step]

theorem extract_only_uploaded (r : revision.Revision) (b : Bool) :
    revision.may_extract r = ok b → (b = true ↔ r.state = .Uploaded) := by
  obtain ⟨st, m, ec, nt⟩ := r
  cases st <;> simp [revision.may_extract]

end Strata
