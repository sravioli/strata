# Spike: Lean + Aeneas on the revision lifecycle (issue #8)

Throwaway prototype. It answers one question: how much of the Rust core can be
proved in Lean through Charon and Aeneas, and what does it cost.

## Layout

| Path | Content |
| --- | --- |
| `domain-core/` | Pure Rust crate. `step`, `searchable`, `new_revision` in `src/lib.rs`. |
| `probes/` | Rust feature probes (strings, closures, iterators, `dyn`, ...). `run.sh` reports which tool accepts each one. |
| `lean/StrataSpike/Model.lean` | Hand-written Lean model and the invariant proofs. Core Lean only. |
| `lean/StrataSpike/Generated/` | Output of Aeneas. Do not edit. Regenerate with `translate.sh`. |
| `lean/StrataSpike/Refinement.lean` | Proof that the generated code agrees with the model, and the invariants restated on the Rust code. |
| `lean/StrataSpike/OverflowProbe.lean` | One integer overflow proof, as a cost sample. |
| `lean/StrataSpike/Audit.lean` | Prints the axioms each main theorem uses. |
| `translate.sh` | Rust -> Charon -> Aeneas -> Lean. |

## Pinned versions

| Tool | Version |
| --- | --- |
| Aeneas | commit `0d21a9ec7bbbb110110cacc14464a47fd71c8705` (`~/source/aeneas`) |
| Charon | commit `454586e9b770231e66d2062993e632dfc06463e2` (from `~/source/aeneas/charon-pin`), version 0.1.315 |
| Rust | `nightly-2026-09-17` (from `charon/charon/rust-toolchain`; set in `domain-core/rust-toolchain`) |
| Lean | `leanprover/lean4:v4.36.0-rc2` (from `backends/lean/lean-toolchain`; set in `lean/lean-toolchain`) |
| Mathlib | `9d581dfff2e41de989072ea63abf3daf69311520` (tag `v4.36.0-rc2`, through the Aeneas library; see `lean/lake-manifest.json`) |
| OCaml | 5.3.0 (to build the Aeneas binary) |

The lake project depends on the Aeneas Lean library by pinned git revision
(`lean/lakefile.toml`), not by local path. A local path dependency makes lake
write build output into `~/source/aeneas`.

## Reproduce

All commands run from the repository root.

1. Charon. Already built at `~/source/aeneas/charon/bin/charon`. If it is not:
   `make -C ~/source/aeneas setup-charon`.

2. Aeneas binary. `~/source/aeneas` did not contain a built binary. The spike
   builds it into `spike/.toolchain/` (ignored by git) and does not write into
   the Aeneas checkout:

   ```sh
   mkdir -p spike/.toolchain && cd spike/.toolchain
   opam switch create . 5.3.0 --no-install -y
   opam install --switch=. -y dune calendar domainslib easy_logging logs menhir \
     ocamlgraph ppx_deriving ppx_deriving_yojson ppx_deriving_hash ppxlib progress \
     unionFind "visitors>=20260520" yojson zarith
   eval $(opam env --switch=. --set-switch)
   dune build --root ~/source/aeneas/src --build-dir "$PWD/aeneas-build" main.exe
   cd ../..
   ```

   The alternative is `make` in `~/source/aeneas` (writes `bin/aeneas` there);
   then set `AENEAS=~/source/aeneas/bin/aeneas`.

3. Rust tests:

   ```sh
   (cd spike/domain-core && cargo test)
   ```

4. Translate Rust to Lean (rewrites `lean/StrataSpike/Generated/*.lean`):

   ```sh
   spike/translate.sh
   ```

5. Check all proofs. The first run downloads the Lean toolchain, the Mathlib
   cache and a prebuilt Aeneas library (about 8 GB in `spike/lean/.lake`):

   ```sh
   cd spike/lean
   lake exe cache get      # first time only
   lake build
   grep -rnw 'sorry\|axiom\|admit' StrataSpike StrataSpike.lean   # expect no output
   ```

   `lake build` must end with `Build completed successfully`. It prints the
   axioms of each main theorem; only `propext`, `Classical.choice` and
   `Quot.sound` are expected.

6. Feature probes (optional):

   ```sh
   spike/probes/run.sh
   ```

## States and events

State: one revision (`number`, `status`) and `active`, the number of the
document's active revision (or none). A larger number is a newer revision.

Status: Uploaded, Processing, Ready, Failed, Deleted.

| Event | Effect | Applies when |
| --- | --- | --- |
| StartProcessing | Uploaded -> Processing | status is Uploaded |
| ProcessingSucceeded | Processing -> Ready | status is Processing |
| ProcessingFailed | Processing -> Failed | status is Processing |
| Activate | `active` := this revision's number | status is Ready and this revision is newer than `active` |
| OtherActivated(n) | `active` := n | n is not this revision and n is newer than `active` |
| Delete | status := Deleted | always |

An event that does not apply leaves the state unchanged. A revision is
searchable if and only if its status is Ready and `active` is its own number.

## Result

See the issue #8 write-up.
