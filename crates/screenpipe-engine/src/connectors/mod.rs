// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Unified connector skeleton for external content channels (S2).
//!
//! A channel implements [`Connector`] — connection lifecycle, scope, sync and
//! search — on top of the shared `connector_*` tables (see
//! `screenpipe_db::connector`). The generic verbs stay in one place so a new
//! channel only writes its own fetch/scope semantics and never grows a second
//! task scheduler or a second source of truth. The office connector predates
//! this trait and keeps its own `/connections/office` surface; adapting it is
//! a mechanical follow-up once a second non-office channel hardens the shape.

use serde_json::json;

pub mod office_adapter;
pub mod rss;
pub mod routes;

/// Channel-agnostic service error carrying an HTTP status for the REST layer.
#[derive(Debug, Clone)]
pub struct ConnectorError {
    pub code: String,
    pub message: String,
    pub transient: bool,
    pub http: u16,
}

impl ConnectorError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, http: u16) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            transient: false,
            http,
        }
    }

    pub fn transient(mut self) -> Self {
        self.transient = true;
        self
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new("invalid_request", message, 400)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new("conflict", message, 409)
    }
}

impl From<sqlx::Error> for ConnectorError {
    fn from(e: sqlx::Error) -> Self {
        Self::new("storage_error", e.to_string(), 500).transient()
    }
}

/// One external content channel. `key` addresses a sub-entry within the
/// channel (RSS has a single `""` key; a multi-account channel would use
/// account ids). Status/search payloads are JSON because each channel's DTO
/// differs; the shared verbs and storage are what this skeleton owns.
#[async_trait::async_trait]
pub trait Connector: Send + Sync {
    fn id(&self) -> &'static str;

    /// Sub-keys exposed by this channel, used by the aggregate index.
    fn keys(&self) -> Vec<String>;

    async fn status(&self, key: &str) -> Result<serde_json::Value, ConnectorError>;

    async fn refresh(&self, key: &str) -> Result<serde_json::Value, ConnectorError>;

    async fn save_scope(
        &self,
        key: &str,
        scope: &serde_json::Value,
    ) -> Result<i64, ConnectorError>;

    async fn start_sync(&self, key: &str, expected_revision: i64) -> Result<i64, ConnectorError>;

    async fn control(&self, key: &str, action: &str) -> Result<(), ConnectorError>;

    async fn disconnect(&self, key: &str, local_data: &str) -> Result<(), ConnectorError>;

    async fn search(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<serde_json::Value, ConnectorError>;
}

/// Aggregate index payload for `/connections/channels`.
pub async fn channels_index(connectors: &[std::sync::Arc<dyn Connector>]) -> serde_json::Value {
    let mut channels = Vec::new();
    for c in connectors {
        let mut entries = Vec::new();
        for key in c.keys() {
            match c.status(&key).await {
                Ok(status) => entries.push(status),
                Err(e) => entries.push(json!({
                    "key": key,
                    "error": { "code": e.code, "message": e.message },
                })),
            }
        }
        channels.push(json!({ "connector": c.id(), "entries": entries }));
    }
    json!({ "channels": channels })
}
