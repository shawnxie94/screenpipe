// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
//
//! Retrieval worker: background dense-indexer and CJK FTS backfiller
//! (SPEC sqlite://artifact/spec/unified-hybrid-retrieval, S2).

use crate::retrieval::embedder::{embed_texts, EmbedderConfig};
use screenpipe_db::DatabaseManager;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Idle period between indexer cycles.
const CYCLE_IDLE: Duration = Duration::from_secs(30);
/// Extra backoff after an embedder failure before the next cycle.
const ERROR_BACKOFF: Duration = Duration::from_secs(60);
/// Transcript rows ingested per cycle.
const TRANSCRIPT_BATCH: i64 = 500;
/// Queue rows embedded per embedder call.
const EMBED_BATCH: i64 = 16;
/// Origin rows reprojected per CJK backfill step.
const CJK_BACKFILL_BATCH: i64 = 500;

/// Spawn the detached retrieval indexer. Everything it does is conditional:
/// without a configured embedding provider the dense legs no-op, and each
/// CJK backfill step turns itself off once its table is covered. Safe to
/// spawn always. `configured` comes from the app settings; env vars remain
/// the dev/CLI fallback.
pub fn spawn_retrieval_indexer(
    db: Arc<DatabaseManager>,
    configured: Option<EmbedderConfig>,
) {
    tokio::spawn(async move {
        info!("retrieval indexer started");
        loop {
            let mut had_error = false;

            // S1: resumable CJK projection backfills, one batch per cycle.
            for (table, step) in [
                ("frames", BackfillStep::Frames),
                ("outputs", BackfillStep::Outputs),
                ("documents", BackfillStep::Documents),
            ] {
                let result = match step {
                    BackfillStep::Frames => db.cjk_backfill_frames_step(CJK_BACKFILL_BATCH).await,
                    BackfillStep::Outputs => db.cjk_backfill_outputs_step(CJK_BACKFILL_BATCH).await,
                    BackfillStep::Documents => db.cjk_reproject_documents_once().await,
                };
                match result {
                    Ok(done) => {
                        debug!(table, done, "cjk backfill step");
                    }
                    Err(error) => {
                        warn!(%error, table, "cjk backfill step failed");
                        had_error = true;
                    }
                }
            }

            // S2: dense leg — only when a provider is configured.
            if let Some(config) = configured.clone().or_else(EmbedderConfig::from_env) {
                match index_cycle(&db, &config).await {
                    Ok(pending) if pending > 0 => {
                        debug!(pending, "retrieval index cycle embedded chunks");
                    }
                    Ok(_) => {}
                    Err(error) => {
                        warn!(%error, "retrieval index cycle failed");
                        had_error = true;
                    }
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(if had_error { ERROR_BACKOFF } else { CYCLE_IDLE }) => {}
            }
        }
    });
}

enum BackfillStep {
    Frames,
    Outputs,
    Documents,
}

/// One drain: enqueue new sources, claim a batch, embed, store.
/// Returns the number of chunks embedded this cycle.
async fn index_cycle(db: &DatabaseManager, config: &EmbedderConfig) -> Result<u64, String> {
    if let Err(error) = db.enqueue_document_chunks().await {
        warn!(%error, "document chunk enqueue failed");
    }
    if let Err(error) = db.enqueue_transcript_chunks(TRANSCRIPT_BATCH).await {
        warn!(%error, "transcript enqueue failed");
    }

    let claimed = db
        .claim_pending_chunks(EMBED_BATCH)
        .await
        .map_err(|e| format!("claim: {e}"))?;
    if claimed.is_empty() {
        return Ok(0);
    }

    let texts: Vec<String> = claimed.iter().map(|c| c.text.clone()).collect();
    let vectors = embed_texts(config, &texts)
        .await
        .map_err(|e| format!("embed: {e}"))?;
    if vectors.len() != claimed.len() {
        for chunk in &claimed {
            let _ = db.fail_queued_chunk(&chunk.chunk_uid).await;
        }
        return Err(format!(
            "embedder returned {} vectors for {} chunks",
            vectors.len(),
            claimed.len()
        ));
    }

    let mut embedded: u64 = 0;
    for (chunk, vector) in claimed.iter().zip(vectors.iter()) {
        if vector.len() != config.dim as usize {
            let _ = db.fail_queued_chunk(&chunk.chunk_uid).await;
            return Err(format!(
                "embedder dim mismatch: got {}, expected {}",
                vector.len(),
                config.dim
            ));
        }
        let meta = chunk.meta();
        match db
            .store_retrieval_embedding(chunk, &config.model, config.dim as usize, vector, &meta)
            .await
        {
            Ok(()) => embedded += 1,
            Err(error) => {
                warn!(%error, chunk_uid = %chunk.chunk_uid, "embedding store failed");
                let _ = db.fail_queued_chunk(&chunk.chunk_uid).await;
            }
        }
    }
    Ok(embedded)
}
