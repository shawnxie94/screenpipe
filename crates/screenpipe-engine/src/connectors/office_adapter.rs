// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Thin `Connector` adapter over the pre-existing `OfficeService`, so the
//! unified channels index treats office and newer channels alike. The office
//! REST surface (`/connections/office/...`) keeps its dedicated handlers;
//! this adapter adds the aggregate view without changing any behavior.

use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::office::{OfficeService, OfficeServiceError};
use crate::connectors::{Connector, ConnectorError};
use screenpipe_connect::office::types::OfficeScope;
use screenpipe_db::DatabaseManager;

pub struct OfficeConnector {
    inner: OfficeService,
}

impl OfficeConnector {
    pub fn new(db: Arc<DatabaseManager>, managed_dir: std::path::PathBuf) -> Self {
        Self {
            inner: OfficeService::new(db, managed_dir),
        }
    }
}

impl From<OfficeServiceError> for ConnectorError {
    fn from(e: OfficeServiceError) -> Self {
        ConnectorError {
            code: e.code.as_str().to_string(),
            message: e.message,
            transient: e.transient,
            http: e.http,
        }
    }
}

#[async_trait::async_trait]
impl Connector for OfficeConnector {
    fn id(&self) -> &'static str {
        "office"
    }

    fn keys(&self) -> Vec<String> {
        vec!["feishu".to_string(), "tencent-meeting".to_string()]
    }

    async fn status(&self, key: &str) -> Result<serde_json::Value, ConnectorError> {
        Ok(serde_json::to_value(self.inner.status(key).await?).unwrap_or(json!(null)))
    }

    async fn refresh(&self, key: &str) -> Result<serde_json::Value, ConnectorError> {
        Ok(serde_json::to_value(self.inner.refresh(key).await?).unwrap_or(json!(null)))
    }

    async fn save_scope(
        &self,
        key: &str,
        scope: &serde_json::Value,
    ) -> Result<i64, ConnectorError> {
        // The office surface PUT body carries expected_revision alongside the
        // scope fields; parse the same shape here.
        #[derive(Deserialize)]
        struct OfficeScopeBody {
            expected_revision: i64,
            #[serde(flatten)]
            scope: OfficeScope,
        }
        let body: OfficeScopeBody = serde_json::from_value(scope.clone())
            .map_err(|e| ConnectorError::bad_request(format!("范围格式无效: {e}")))?;
        Ok(self
            .inner
            .save_scope(key, body.expected_revision, body.scope)
            .await?)
    }

    async fn start_sync(&self, key: &str, expected_revision: i64) -> Result<i64, ConnectorError> {
        Ok(self.inner.start_sync(key, expected_revision).await?)
    }

    async fn control(&self, key: &str, action: &str) -> Result<(), ConnectorError> {
        match action {
            "pause" | "cancel" => {
                // The in-process sync task checks the cancel token; a running
                // task stops at its next cancellation point.
                self.inner.cancel.child_token().cancel();
                Ok(())
            }
            "retry" => {
                self.inner.refresh(key).await?;
                Ok(())
            }
            other => Err(ConnectorError::bad_request(format!(
                "未知操作 {other}（支持 pause/cancel/retry）"
            ))),
        }
    }

    async fn disconnect(&self, key: &str, local_data: &str) -> Result<(), ConnectorError> {
        Ok(self.inner.disconnect(key, local_data).await?)
    }

    async fn search(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<serde_json::Value, ConnectorError> {
        let rows = self.inner.search(None, query, limit).await?;
        Ok(serde_json::to_value(rows).unwrap_or(json!([])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn connector() -> OfficeConnector {
        let db = DatabaseManager::new("sqlite::memory:", Default::default())
            .await
            .unwrap();
        OfficeConnector::new(Arc::new(db), std::env::temp_dir())
    }

    #[tokio::test]
    async fn adapter_exposes_office_providers_as_keys() {
        let c = connector().await;
        assert_eq!(c.id(), "office");
        assert_eq!(c.keys(), vec!["feishu", "tencent-meeting"]);
    }

    #[tokio::test]
    async fn adapter_status_serializes_dto_and_rejects_unknown_provider() {
        let c = connector().await;
        let status = c.status("feishu").await.unwrap();
        assert_eq!(status["provider"], "feishu");
        assert!(status["auth_status"].is_string());

        let err = c.status("unknown-provider").await.unwrap_err();
        assert_eq!(err.http, 400);
    }

    #[tokio::test]
    async fn adapter_error_carries_office_code() {
        let c = connector().await;
        // Sync without a saved scope fails with the office error code mapped
        // through the adapter.
        let err = c.start_sync("feishu", 0).await.unwrap_err();
        assert_eq!(err.code, "scope_invalid");
    }

    #[tokio::test]
    async fn adapter_control_rejects_unknown_action() {
        let c = connector().await;
        let err = c.control("feishu", "reboot").await.unwrap_err();
        assert_eq!(err.http, 400);
    }
}
