# Spec Delta

## ADDED Requirements

### Requirement: The setup builds the pinned nsjail

`make setup` SHALL build nsjail 3.6 from source at the pinned commit `f78475530b46d0186111a9096b30725f816b55fe`, into `.tools/`. The build SHALL stop when the commit of the source is not the pin. `make setup` SHALL warn when `pdftotext` is missing. The `l1` job of CI SHALL use the same build.

#### Scenario: The pinned build

- **WHEN** `make setup` runs on a system with the build dependencies of nsjail
- **THEN** `.tools/` holds an nsjail binary built from the pinned commit

#### Scenario: A source that is not the pin

- **GIVEN** a source of nsjail at a commit that is not the pin
- **WHEN** the build script runs
- **THEN** it stops with an error, and it installs no binary

### Requirement: CI runs the sandbox tests

The `l1` job of CI SHALL run the L1 tests of the sandbox with nsjail. If the runner blocks the user namespaces of nsjail, the job SHALL load an AppArmor profile that permits user namespaces for the nsjail binary only. The job SHALL NOT set `kernel.apparmor_restrict_unprivileged_userns` to 0.

#### Scenario: The sandbox tests run in CI

- **WHEN** CI runs the `l1` job for a pull request
- **THEN** the L1 tests of the sandbox run with nsjail, and none of them skips

#### Scenario: No global change to the restriction

- **WHEN** a test reads `.github/workflows/ci.yml` and the scripts that it runs
- **THEN** no step sets `kernel.apparmor_restrict_unprivileged_userns` to 0
