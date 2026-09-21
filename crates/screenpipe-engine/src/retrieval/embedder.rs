// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
//
//! OpenAI-compatible embedding API client (SPEC S2: the dense leg uses a
//! user-configured provider — cloud gateway or a local OpenAI-compatible
//! server; the app ships no built-in embedder).

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Provider configuration resolved from environment variables:
/// `SCREENPIPE_EMBEDDING_BASE_URL` (e.g. http://host:3000/v1),
/// `SCREENPIPE_EMBEDDING_API_KEY`, `SCREENPIPE_EMBEDDING_MODEL`,
/// `SCREENPIPE_EMBEDDING_DIM`. All must be present for the dense leg to run.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbedderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub dim: u32,
}

impl EmbedderConfig {
    /// Provider configuration from the app settings store. The dense leg
    /// only arms when the user enabled it AND filled the endpoint + key —
    /// the privacy boundary documented in the settings UI.
    pub fn from_settings(settings: &screenpipe_config::RecordingSettings) -> Option<Self> {
        if !settings.embedding_enabled {
            return None;
        }
        let base_url = settings.embedding_base_url.trim().trim_end_matches('/').to_string();
        let api_key = settings.embedding_api_key.clone();
        if base_url.is_empty() || api_key.is_empty() {
            return None;
        }
        Some(Self {
            base_url,
            api_key,
            model: if settings.embedding_model.trim().is_empty() {
                "qwen3-embedding-0.6b-8bit".to_string()
            } else {
                settings.embedding_model.trim().to_string()
            },
            dim: settings.embedding_dim,
        })
    }

    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("SCREENPIPE_EMBEDDING_BASE_URL").ok()?;
        let api_key = std::env::var("SCREENPIPE_EMBEDDING_API_KEY").ok()?;
        let model = std::env::var("SCREENPIPE_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "qwen3-embedding-0.6b-8bit".to_string());
        let dim = std::env::var("SCREENPIPE_EMBEDDING_DIM")
            .ok()
            .and_then(|d| d.parse().ok())
            .unwrap_or(1024);
        let base_url = base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() || api_key.is_empty() {
            return None;
        }
        Some(Self {
            base_url,
            api_key,
            model,
            dim,
        })
    }
}

#[derive(Serialize)]
struct EmbeddingsRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingsData>,
}

#[derive(Deserialize)]
struct EmbeddingsData {
    #[serde(default)]
    index: usize,
    embedding: Vec<f32>,
}

const HTTP_TIMEOUT: Duration = Duration::from_secs(60);

/// Embed `texts` in one batched call. Vector order matches input order
/// (falls back to the returned `index` field when the provider reorders).
pub async fn embed_texts(
    config: &EmbedderConfig,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!("{}/embeddings", config.base_url);
    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let response = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .json(&EmbeddingsRequest {
            model: &config.model,
            input: texts,
        })
        .send()
        .await
        .map_err(|e| format!("POST {url}: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(200).collect();
        return Err(format!("embeddings HTTP {status}: {snippet}"));
    }

    let parsed: EmbeddingsResponse = response
        .json()
        .await
        .map_err(|e| format!("decode embeddings response: {e}"))?;
    if parsed.data.len() != texts.len() {
        return Err(format!(
            "embeddings returned {} vectors for {} inputs",
            parsed.data.len(),
            texts.len()
        ));
    }
    let mut vectors: Vec<Option<Vec<f32>>> = (0..texts.len()).map(|_| None).collect();
    for item in parsed.data {
        if item.index < texts.len() {
            vectors[item.index] = Some(item.embedding);
        }
    }
    Ok(vectors
        .into_iter()
        .enumerate()
        .map(|(i, v)| v.ok_or_else(|| format!("embeddings missing vector at index {i}")))
        .collect::<Result<Vec<_>, String>>()?)
}
