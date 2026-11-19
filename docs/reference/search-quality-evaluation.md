# Search quality evaluation

> Background reading. This file comes from the handoff package of 2026-11-14 (issue [#10](https://github.com/sravioli/strata/issues/10)). It was written before any decision. Where it differs from an accepted ADR in `docs/adr/`, the ADR is correct.

## Evaluation corpus

Build a representative, permission-labeled corpus with multiple tenants, document types, languages and document sizes. Create query sets including exact phrase, natural-language, typo, partial term, semantic paraphrase, multilingual and ACL-restricted cases. Expected relevant documents/chunks must be labeled by reviewers or a controlled relevance process.

## Metrics

- Recall@K and Precision@K.
- Mean Reciprocal Rank (MRR).
- nDCG@K.
- Search latency p50/p95/p99.
- Tenant/ACL violation count (must be zero).
- Vector candidate recall under tenant filters.
- Search index size and update/rebuild time.

## Comparisons

Compare FTS, trigram, vector and hybrid RRF. Tune candidate depth, RRF constant, FTS weights, trigram thresholds and HNSW settings on a training set; report results on a held-out evaluation set. Segment by language, tenant size and query type.

## Change control

Every parser/chunker/embedding model/search ranking change records version, evaluation results and rollback method. Keep query and document embedding generations compatible. Do not re-embed all historical documents implicitly during a query-model deployment; use explicit migration generations.
