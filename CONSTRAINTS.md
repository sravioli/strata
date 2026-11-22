# Constraints

The quality bar for every change. CI enforces each row of "Enforced" through the `ci-ok` check, and the "Protect main" ruleset requires `ci-ok` before a merge.

Last reviewed: 2026-11-20 by the owner.

## Floor

Review checks these rules. No tool checks them yet.

- A change adds no new suppression: `#[allow(...)]`, `#[ignore]`, `# noqa`, `# type: ignore`, `// nosemgrep`, or a new entry in `.betterleaksignore`, unless the commit message gives the reason.
- A change adds no unfinished work: `todo!()`, `unimplemented!()`, a `TODO` in place of the code, or an empty error branch.
- A change does not skip or delete a test, unless the commit message gives the reason. The tests that skip without Charon and Aeneas are the only exception: CI sets `REQUIRE_PROOF_TOOLS`, so they cannot skip there.
- A change lowers no number in this file. Only the owner lowers a number, in a pull request that changes this file and nothing else.

## Enforced

| Dimension | Rule | Checked by | Runs at |
|---|---|---|---|
| Format | No diff | `cargo fmt --all --check` | `make rust`, CI `rust` |
| Lint | No warnings | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | `make rust`, CI `rust` |
| Coverage | Lines of the workspace at 80% or more (L0 and L1 tests) | `cargo llvm-cov --workspace --locked --all-features --fail-under-lines 80` | `make l1`, CI `l1` |
| Dependencies | No advisory, licence or source failure | `cargo deny check` (`deny.toml`) | `make deny`, CI `deny` |
| Secrets | No finding | `betterleaks git .` | `make secrets`, CI `secrets` |
| Code scanning | CodeQL and zizmor: no alert at the threshold of the "Protect main" ruleset | the "Protect main" ruleset | each pull request |
| Test cases | Each ID has a test or a PENDING marker | `python3 scripts/check-test-cases.py` | `make test-cases`, CI `test-cases` |
| Proofs | The checks of tech design 0009 | `scripts/translate.sh --check`, `scripts/check-proofs.py` | `make verify`, CI `verify` |
| Commits | Conventional commits | `cog check` | `make commits`, CI `commits` |

### Coverage

- **The number.** 80 is the value of the workspace on 2026-11-21 (80.34%) with the L0 tests, rounded down. The owner kept 80 when the scope changed. With the L0 and L1 tests, the value is 86.52% on 2026-11-22. It holds the current line. It is not a target.
- **The ratchet.** When a change raises the coverage by one point or more, the same change raises the number to the new value, rounded down.
- **The scope.** The number counts the L0 and L1 tests together, in the `l1` job of each pull request. The owner moved the scope from L0 to L0 and L1 on 2026-11-22 (#49). The reason: the database and HTTP code of the crate `strata` runs only in L1 tests. With L0 tests only, the workspace was at 47%.
- **Known gap.** Six arms of `job_step` (in `crates/core/src/job.rs`) have no L0 test. The Lean proofs cover them, but no Rust test runs them.

## Exceptions

None.
