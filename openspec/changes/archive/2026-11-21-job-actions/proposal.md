Issue: #69

# Proposal

## Why

`DeleteDocument` creates a `RemoveFiles` job (tech design 0006). A job can get "allow" only for `ProcessDocument` (tech design 0004), so a `RemoveFiles` job has no action. On 2026-11-21 the owner approved a second job action, `RemoveDocumentFiles`, and the new statements of two theorems. This change makes the Rust code and the proofs agree with them.

## What Changes

- Add the action `RemoveDocumentFiles` to the core crate. Only a job can get "allow" for it.
- Change the rule of `authorize` and `authorized_scope`:
  - An account can get "allow" only for an account action: `Search`, `ReadDocument`, `UploadDocument` or `DeleteDocument`.
  - A job can get "allow" only for a job action: `ProcessDocument` or `RemoveDocumentFiles`.
- Change the Lean model and the proofs of `account_never_processes` and `job_only_processes` to the statements in the "Proof obligations" table of 0004. The other theorems of 0004 keep their statements.
- Translate the core crate again (`lean/Strata/Generated/`).

Not in this change: the `RemoveFiles` job, its SQL and the cleanup task. Issue #69 designs them.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. The change sets `skip_specs: true`. The behavior is in the theorems of 0004. The "Proof obligations" table of an accepted design is the only list of theorems (tech design 0009, V6). A spec that repeats the theorems is a second list.

## Impact

- `crates/core/src/authorization.rs`: one new variant of `Action`, and the rule in `action_allowed`.
- `lean/Strata/Model/Authorization.lean` and `lean/Strata/Proofs/Authorization.lean`: the new action and the two new statements.
- `lean/Strata/Generated/`: a new translation.
- No API, no table and no migration changes.
