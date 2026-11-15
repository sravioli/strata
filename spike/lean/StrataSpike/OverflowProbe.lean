/-
Probe: what an integer overflow proof obligation looks like.
Source: `spike/probes/src/lib.rs`, `next_number_plain`.
Not part of the lifecycle slice.
-/
import Aeneas
import StrataSpike.Generated.Probes

open Aeneas Std Result

namespace Strata.OverflowProbe

/-- If `latest` is below `u32::MAX`, the Rust `latest + 1` does not panic and
returns `latest + 1`. -/
theorem next_number_plain_spec (latest : U32) (h : latest.val < U32.max) :
    probes.next_number_plain latest ⦃ r => r.val = latest.val + 1 ⦄ := by
  unfold probes.next_number_plain
  step as ⟨ r ⟩
  scalar_tac

end Strata.OverflowProbe
