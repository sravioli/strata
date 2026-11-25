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
| [0009](0009-polled-revision-status.md) | Document readiness | [0047](../tech-design/0047-document-readiness.md) | [#47](https://github.com/sravioli/strata/issues/47) |
| [0010](0010-generated-hostile-input-corpus.md) | Hostile-input corpus | [0048](../tech-design/0048-hostile-input-corpus.md) | [#48](https://github.com/sravioli/strata/issues/48) |
| [0011](0011-walking-skeleton-postgres-search.md) | Walking skeleton | [0049](../tech-design/0049-walking-skeleton.md) | [#49](https://github.com/sravioli/strata/issues/49) |
| [0012](0012-local-observability-opentelemetry.md) | Observability, local | [0133](../tech-design/0133-local-observability.md) | [#133](https://github.com/sravioli/strata/issues/133) |
| [0013](0013-browser-clients-exact-cors-origins.md) | Browser clients | [0065](../tech-design/0065-demo-frontend.md) | [#65](https://github.com/sravioli/strata/issues/65) |

Copy [0000-template.md](0000-template.md) for a new ADR, and add a row here in the same pull request.
