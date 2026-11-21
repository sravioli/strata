# Architecture decision records

The ADR numbers and the tech design numbers are different. A tech design has the number of its issue. An ADR has the next number in sequence.

| ADR | Subject | Tech design | Issue |
|---|---|---|---|
| [0001](0001-shared-schema-tenancy.md) | Tenancy | [0003](../tech-design/0003-tenancy-model.md) | [#3](https://github.com/sravioli/strata/issues/3) |
| [0002](0002-single-decision-function-authorization.md) | Authorization | [0004](../tech-design/0004-v1-authorization.md) | [#4](https://github.com/sravioli/strata/issues/4) |
| [0003](0003-capacity-ceiling-and-stateless-scaling.md) | Capacity | [0002](../tech-design/0002-capacity-ceiling.md) | [#2](https://github.com/sravioli/strata/issues/2) |
| [0004](0004-postgresql-job-queue.md) | Job queue | [0005](../tech-design/0005-job-queue.md) | [#5](https://github.com/sravioli/strata/issues/5) |
| [0005](0005-s3-api-object-storage.md) | Object storage | [0006](../tech-design/0006-object-storage.md) | [#6](https://github.com/sravioli/strata/issues/6) |
| [0006](0006-sandboxed-extraction-process.md) | Extraction sandbox | [0007](../tech-design/0007-extraction-sandbox.md) | [#7](https://github.com/sravioli/strata/issues/7) |
| [0007](0007-verification-boundary.md) | Verification boundary | [0009](../tech-design/0009-verification-boundary.md) | [#9](https://github.com/sravioli/strata/issues/9) |
| [0008](0008-trusted-external-issuers.md) | Identity providers | [0021](../tech-design/0021-identity-providers.md) | [#21](https://github.com/sravioli/strata/issues/21) |

Copy [0000-template.md](0000-template.md) for a new ADR, and add a row here in the same pull request.
