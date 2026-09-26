# Retrieval evaluation

This benchmark measures **retrieval**, not answer generation or Agent behavior. The canonical first-stage suite is the purpose-created, deterministic fixture in `crates/screenpipe-engine/tests/retrieval_eval_test.rs`.

## Run

```bash
cargo test -p screenpipe-engine --test retrieval_eval_test -- --nocapture
```

The retrieval test creates a temporary SQLite database and deterministic hash vectors. It needs no network, credentials, or production user database. Output reports per-query and aggregate baseline/hybrid Recall@10, baseline/hybrid MRR@10, hybrid per-category Recall@10/MRR@10, and cross-source evidence coverage. The endpoint filter regression is run separately:

```bash
cargo test -p screenpipe-engine --test endpoint_test unified_search_applies_source_app_and_time_filters_before_pagination
```

It confirms `/search/records` applies OCR content type, app name, and time-range filters before returning the page.

An optional live smoke probe can be run against a purpose-created synthetic local database:

```bash
bun scripts/retrieval-eval.ts --base http://localhost:3030
```

It reads `data[]` in keyword and relevance modes, reports selector coverage/MRR, and checks relevance degradation status. These are **exploratory text-selector metrics**, not labeled-record Recall@K: user databases are not stable golden corpora, and no query/result content is persisted. Do not use its numbers as a quality threshold or share output containing user context.

## Case and labeling format

Each `GoldenQuery` is a labeled case:

- `id`: stable case identifier; do not recycle IDs.
- `category`: slice used in the report (`zh-document`, `zh-audio`, `zh-ocr`, `en-ocr`, `cross-source`).
- `query`: purpose-created query text, not copied from a user's history.
- `relevant`: every relevant `(source_type, source_pk)` key. For a cross-source question, list all required evidence items; recall then measures evidence-set coverage.

Labels are binary (relevant/not relevant), and a test rejects empty IDs, queries, or relevance selectors. The suite is intentionally a small regression seed, not a representative estimate of production quality. Add cases for a concrete failure mode and keep the fixture corpus and relevant keys deterministic.

## Agent-facing result envelope

The search API wire format remains unchanged. MCP stdio and HTTP tool calls expose an additive `structuredContent` object; the bundled Pi search extension returns the same versioned envelope as bounded JSON text because its tool API is string-returning. Both retain a human-readable summary/text for clients that do not consume structured fields.

The `screenpipe.search-results.v1` envelope contains:

- `schema`, `schema_version`, `summary`, `mode`, `degraded`, `legs_used`, `warnings`, and `pagination` (`limit`, `offset`, `total`).
- `results[]`: stable `id`, `source_type`, `source_id`, one-based `rank`, bounded `text` excerpt, optional event `timestamp`, `app`, `window_name`, whitelisted `source_ref` identifiers, and `retrieval` (`score`, `matched_legs`). Each item and the envelope indicate truncation.
- `warnings` surfaces hybrid degradation and response-budget truncation. `degraded`/`legs_used` preserve retrieval state even when no result is returned.

`source_ref` is limited to stable source identifiers such as frame/chunk IDs, document hash/ordinal, and connector object identifiers. The envelope does not include embeddings, tokens, absolute paths, or unrestricted source payloads. Scores are ranking diagnostics, not calibrated confidence. Clients must use the stable source keys—not display text or rank—as evidence identity.

## Metrics

- **Recall@10**: fraction of labeled relevant items found in the first 10 candidates.
- **MRR@10**: reciprocal rank of the first relevant candidate; zero if absent.
- **Cross-source evidence coverage@10**: recall over the complete evidence set for cross-source cases.
- The existing hybrid-vs-time-ordered assertion is a regression comparison, not an absolute quality target.

Metric helpers have hand-calculated unit examples. The empty-dense-index test verifies sparse results survive when no vectors are available. This retrieval-layer check does not validate HTTP degraded metadata or every API filter.

## Privacy and interpretation

Only synthetic fixture content and stable synthetic IDs belong here. Never copy screen captures, OCR text, transcripts, document bodies, personal paths, or identifiers from a user's database into this repository. If a realistic example cannot be safely purpose-created, omit it rather than attempting weak anonymization.

Hashing vectors are a deterministic test double and do not estimate the quality of any configured embedding model. Do not compare their scores with a live provider or describe this suite as production-quality evaluation. A separate, explicitly authorized, synthetic-only provider evaluation may be added later.

## Known gaps / follow-up

- Small coverage and binary labels; grow a human-reviewed query set before setting score thresholds.
- Structured-filter coverage currently checks one unified OCR query across content type, app name, and time range. Extend it with additional source types and boundary cases; API-level degradation/fallback metadata remains only partly covered.
- The result envelope is an additive Agent-boundary contract; changing the engine/API's native response schema requires its own compatibility review.
- Agentic RAG needs a separate task suite with expected evidence, allowed retrieval actions, stopping conditions, and task-completion grading. This benchmark does not evaluate an Agent loop.
