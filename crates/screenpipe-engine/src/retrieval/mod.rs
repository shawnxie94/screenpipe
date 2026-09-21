// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
//
//! Unified hybrid retrieval (SPEC sqlite://artifact/spec/unified-hybrid-retrieval):
//! - `embedder`: OpenAI-compatible embedding API client;
//! - `worker`: background dense-indexer + CJK FTS backfill driver;
//! - `fusion`: RRF/time-decay ranking primitives (pure, unit-tested); the
//!   relevance handler lives in `routes::search` beside the legacy handler.

pub mod embedder;
pub mod fusion;
pub mod worker;

pub use worker::spawn_retrieval_indexer;
