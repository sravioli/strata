/-
Model of the revision state function (tech design 0049, "The lifecycle of a revision").
The Lean statements of the "Proof obligations" table of 0049 are about these definitions.
`Strata/Proofs/Revision.lean` restates them on the Rust code.
-/

namespace Strata.Model

inductive RevisionState where
  | awaitingUpload
  | uploaded
  | ready
  | failed
  | superseded
  deriving DecidableEq

/-- `FileTooLarge` of 0006, or a class of 0007. -/
inductive FailureClass where
  | fileTooLarge
  | extractionTimeout
  | extractionMemory
  | extractionCrash
  | extractionOutputTooLarge
  | archiveTooLarge
  | unsupportedFormat
  | encryptedFile
  | malformedFile
  | invalidOutput
  deriving DecidableEq

structure Revision where
  state : RevisionState
  retryMark : Bool
  errorClass : Option FailureClass
  noText : Bool

inductive RevisionEvent where
  | finish
  | finishTooLarge
  | extracted (noText : Bool)
  | extractFailed (c : FailureClass)
  | newerReady
  deriving DecidableEq

/-- A crash, the time limit or invalid output retries one time (0007). -/
def retries : FailureClass → Bool
  | .extractionCrash | .extractionTimeout | .invalidOutput => true
  | _ => false

/-- The table of events of 0049. Each other pair leaves the revision as it is. -/
def revisionStep (r : Revision) (e : RevisionEvent) : Revision :=
  match r.state, e with
  | .awaitingUpload, .finish => { r with state := .uploaded }
  | .awaitingUpload, .finishTooLarge => { r with state := .failed, errorClass := some .fileTooLarge }
  | .uploaded, .extracted n => { r with state := .ready, noText := n }
  | .uploaded, .extractFailed c =>
    if retries c && !r.retryMark then { r with retryMark := true }
    else { r with state := .failed, errorClass := some c }
  | .awaitingUpload, .newerReady | .uploaded, .newerReady | .ready, .newerReady =>
    { r with state := .superseded }
  | _, _ => r

def mayExtract (r : Revision) : Bool := r.state = .uploaded

/-! ## The statements of the 0049 table, on the model -/

theorem final_sticky (r : Revision) (e : RevisionEvent) :
    (r.state = .failed ∨ r.state = .superseded) → revisionStep r e = r := by
  rintro (h | h) <;> cases e <;> simp [revisionStep, h]

theorem ready_only_superseded (r : Revision) (e : RevisionEvent) :
    r.state = .ready → (revisionStep r e).state = .ready ∨ (revisionStep r e).state = .superseded := by
  intro h; cases e <;> simp [revisionStep, h]

theorem retry_once (r : Revision) (c : FailureClass) :
    r.state = .uploaded → retries c = true →
      (r.retryMark = false →
        (revisionStep r (.extractFailed c)).state = .uploaded ∧
        (revisionStep r (.extractFailed c)).retryMark = true) ∧
      (r.retryMark = true →
        (revisionStep r (.extractFailed c)).state = .failed ∧
        (revisionStep r (.extractFailed c)).errorClass = some c) := by
  intro hs hc
  constructor <;> intro hm <;> simp [revisionStep, hs, hc, hm]

theorem extract_only_uploaded (r : Revision) : mayExtract r = true ↔ r.state = .uploaded := by
  simp [mayExtract]

end Strata.Model
