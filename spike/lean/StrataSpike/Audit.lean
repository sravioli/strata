/-
Prints the axioms each main theorem depends on. `lake build` shows the output.
Expected: only Lean's standard axioms (`propext`, `Classical.choice`,
`Quot.sound`). `sorryAx` in this list means an unfinished proof.
-/
import StrataSpike.Refinement

open Strata

#print axioms Model.searchable_iff
#print axioms Model.becomes_searchable_only_by_activate
#print axioms Model.run_wf
#print axioms Model.older_never_replaces_newer
#print axioms Model.after_delete_never_searchable
#print axioms Model.step_idempotent
#print axioms Refinement.step_refines
#print axioms Refinement.searchable_refines
#print axioms Refinement.new_revision_refines
#print axioms Refinement.runsTo_total
#print axioms Refinement.run_refines
#print axioms Refinement.rust_searchable_iff
#print axioms Refinement.rust_becomes_searchable_only_by_activate
#print axioms Refinement.rust_new_revision_wf
#print axioms Refinement.rust_run_wf
#print axioms Refinement.rust_older_never_replaces_newer
#print axioms Refinement.rust_after_delete_never_searchable
#print axioms Refinement.rust_step_idempotent
