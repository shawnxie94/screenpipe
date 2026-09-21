// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
//
//! Hybrid fusion (SPEC S3): reciprocal-rank fusion over the sparse FTS legs
//! and the dense KNN leg, with an optional time-decay prior. The pure
//! functions here are the only ranking logic; the handler just gathers legs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::retrieval::embedder::{embed_texts, EmbedderConfig};
use crate::AppState;
use chrono::Utc;
use screenpipe_db::{ContentType, Order, SearchResult};

/// RRF smoothing constant. Rank-only fusion needs no score calibration
/// between BM25 and cosine (SPEC decision: RRF over linear weighting).
pub const RRF_K: f32 = 60.0;
/// Default time-decay half-life in days; configurable per request.
pub const DEFAULT_TAU_DAYS: f32 = 30.0;
/// Weight of the decay bonus relative to the RRF score.
pub const DEFAULT_LAMBDA: f32 = 0.05;

/// One leg's ranked candidate keys, best first. Keys share the candidate
/// projection space: `"<source_type>:<source_pk>"`.
pub type RankedLeg<'a> = (&'a str, Vec<String>);

/// Fuse ranked legs: each candidate scores `sum(1/(k + rank))` over the legs
/// it appears in. Returns candidates sorted by descending score, with the
/// contributing leg names attached.
pub fn rrf_fuse(legs: Vec<RankedLeg>, k: f32) -> Vec<(String, f32, Vec<String>)> {
    let mut scores: HashMap<String, (f32, Vec<String>)> = HashMap::new();
    for (leg_name, candidates) in &legs {
        for (position, key) in candidates.iter().enumerate() {
            let entry = scores.entry(key.clone()).or_insert_with(|| (0.0, Vec::new()));
            entry.0 += 1.0 / (k + position as f32 + 1.0);
            if !entry.1.iter().any(|l| l == leg_name) {
                entry.1.push((*leg_name).to_string());
            }
        }
    }
    let mut fused: Vec<(String, f32, Vec<String>)> = scores
        .into_iter()
        .map(|(key, (score, legs))| (key, score, legs))
        .collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused
}

/// Exponential decay bonus: `exp(-age_seconds / tau_seconds)`, 1.0 for
/// brand-new content, ~0 beyond the horizon.
pub fn decay_bonus(age_seconds: f64, tau_seconds: f64) -> f32 {
    if age_seconds <= 0.0 {
        return 1.0;
    }
    (-age_seconds / tau_seconds.max(1.0)).exp() as f32
}

/// A candidate with the timestamp needed to apply the decay prior.
#[derive(Debug, Clone)]
pub struct Timestamped {
    pub key: String,
    pub ts_epoch: Option<f64>,
}

/// Final ranking: RRF score (normalized against the fused maximum) plus the
/// decay bonus so recent hits edge out equally-fused stale ones without
/// letting recency dominate a strong relevance gap.
pub fn rank_with_decay(
    fused: Vec<(String, f32, Vec<String>)>,
    timestamps: &HashMap<String, f64>,
    now_epoch: f64,
    lambda: f32,
    tau_days: f32,
) -> Vec<(String, f32, Vec<String>)> {
    let max = fused
        .iter()
        .map(|(_, s, _)| *s)
        .fold(f32::MIN_POSITIVE, f32::max);
    let tau = tau_days * 86400.0;
    let mut ranked: Vec<(String, f32, Vec<String>)> = fused
        .into_iter()
        .map(|(key, score, legs)| {
            let age = timestamps.get(&key).map(|ts| now_epoch - *ts).unwrap_or(0.0);
            let bonus = lambda * decay_bonus(age, tau as f64);
            let normalized = if max > 0.0 { score / max } else { score };
            (key, normalized + bonus, legs)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
}

/// Sorting mode for /search. `Time` is the legacy behavior and the default;
/// `Relevance` opts into hybrid fusion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    #[default]
    Time,
    Relevance,
}

/// One ranked hit of the hybrid response. Compact by design: the unified
/// chat tool consumes this directly, and each hit carries its projection
/// contract fields so callers can jump back to the origin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridHit {
    pub source_type: String,
    pub source_pk: String,
    pub score: f32,
    pub legs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResponse {
    pub results: Vec<HybridHit>,
    /// True when the dense leg could not run (provider unconfigured or
    /// failed) — the response then reflects sparse legs only.
    pub degraded: bool,
    pub legs_used: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_prefers_multi_leg_hits() {
        let legs = vec![
            ("ocr", vec!["ocr:7".into(), "ocr:3".into()]),
            ("transcript", vec!["transcript:1".into(), "ocr:7".into()]),
        ];
        let fused = rrf_fuse(legs, RRF_K);
        let top = &fused[0];
        assert_eq!(top.0, "ocr:7");
        assert_eq!(top.2, vec!["ocr", "transcript"]);
        assert!(top.1 > fused[1].1);
    }

    #[test]
    fn decay_prefers_recent_but_respects_fusion() {
        let now = 1_800_000_000.0;
        let timestamps = HashMap::from([
            ("old".to_string(), now - 300.0 * 86400.0),
            ("new".to_string(), now - 60.0),
        ]);
        let fused = vec![
            ("old".to_string(), 0.02, vec!["ocr".into()]),
            ("new".to_string(), 0.015, vec!["ocr".into()]),
        ];
        let ranked = rank_with_decay(fused, &timestamps, now, 0.3, 30.0);
        assert_eq!(ranked[0].0, "new", "large lambda: recency flips close scores");
    }

    #[test]
    fn strong_relevance_beats_decay() {
        let now = 1_800_000_000.0;
        let timestamps = HashMap::from([
            ("old_strong".to_string(), now - 300.0 * 86400.0),
            ("new_weak".to_string(), now - 10.0),
        ]);
        let fused = vec![
            ("old_strong".to_string(), 0.05, vec!["ocr".into()]),
            ("new_weak".to_string(), 0.01, vec!["ocr".into()]),
        ];
        let ranked = rank_with_decay(fused, &timestamps, now, DEFAULT_LAMBDA, DEFAULT_TAU_DAYS);
        assert_eq!(ranked[0].0, "old_strong", "gentle default decay keeps relevance on top");
    }

    #[test]
    fn decay_bonus_bounds() {
        assert_eq!(decay_bonus(0.0, 100.0), 1.0);
        assert!(decay_bonus(-5.0, 100.0) == 1.0);
        assert!(decay_bonus(100.0, 100.0) < 0.5);
    }
}
