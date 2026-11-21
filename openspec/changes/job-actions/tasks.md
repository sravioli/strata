# Tasks

## 1. Rust

- [ ] 1.1 In `crates/core/src/authorization.rs`, add the variant `RemoveDocumentFiles` to `Action`. In `action_allowed`, an account gets only the account actions, and a job gets only `ProcessDocument` and `RemoveDocumentFiles`. Keep the code in the proved subset of 0009: enums, `match` and `matches!`. Verify: `cargo build -p strata-core` passes on stable.
- [ ] 1.2 Add L0 tests. A job of an active tenant gets "allow" for `RemoveDocumentFiles` on a resource of its tenant. It gets "deny" with `OtherTenant` for a resource of a different tenant. An active account gets "deny" with `NotPermitted` for `RemoveDocumentFiles`. `authorized_scope` agrees with each case. Verify: `cargo test -p strata-core` passes, and each new test fails when the rule of 1.1 is removed.

## 2. Lean

- [ ] 2.1 In `lean/Strata/Model/Authorization.lean`, add `removeDocumentFiles` to `Action`, set `forAccount` to `false` for it, and permit a job only the actions with `forAccount` set to `false`. Change the model theorems `account_never_processes` and `job_only_processes` to the Lean statements of the 0004 table. Verify: `lake build` passes.
- [ ] 2.2 Run `scripts/translate.sh` with the pinned Charon and Aeneas. Do not edit `lean/Strata/Generated/` by hand. Verify: `scripts/translate.sh --check` prints `ok`.
- [ ] 2.3 In `lean/Strata/Proofs/Authorization.lean`, update `absAction` for the new variant. Restate `account_never_processes` and `job_only_processes` on the Rust code, with the statements of the 0004 table. Keep `authorize_refines`, `authorized_scope_refines` and `authorize_total`. Verify: `lake build` passes with no `sorry`, and `python3 scripts/check-proofs.py` passes.

## 3. Quality gates

- [ ] 3.1 Run the gates of `CONSTRAINTS.md`: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, `cargo llvm-cov --workspace --locked --fail-under-lines 75`, `python3 scripts/check-test-cases.py` and `python3 -m unittest discover -s scripts/tests`. Verify: each command passes.

## Workflow follow-up

- Archive the change with `/openspec-archive-change` after the pull request is merged.
