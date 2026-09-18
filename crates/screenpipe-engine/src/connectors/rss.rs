// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! RSS/Atom feed channel — the first implementation on the generic connector
//! skeleton. Connection = the configured feed list (no external identity),
//! scope = the feed URLs, sync = fetch + parse + upsert into the shared
//! `connector_*` store with per-feed cursors, search = shared FTS.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use screenpipe_db::{ConnectorConnectionUpdate, ConnectorObjectDraft, DatabaseManager};

use super::ConnectorError;

pub const CONNECTOR_ID: &str = "rss";
const KEY: &str = "rss";
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

/// User-approved scope: the feed list plus the auto-sync toggle. Empty list =
/// nothing to sync.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RssScope {
    #[serde(default)]
    pub feed_urls: Vec<String>,
    #[serde(default)]
    pub auto_sync: bool,
}

impl RssScope {
    fn is_empty(&self) -> bool {
        self.feed_urls.is_empty()
    }
}

pub struct RssService {
    pub db: Arc<DatabaseManager>,
    pub cancel: CancellationToken,
}

impl RssService {
    pub fn new(db: Arc<DatabaseManager>) -> Self {
        Self {
            db,
            cancel: CancellationToken::new(),
        }
    }

    fn scope_from_json(json: &str) -> RssScope {
        serde_json::from_str::<RssScope>(json).unwrap_or_default()
    }

    /// Aggregate status for the connection card. Derived auth: the channel has
    /// no external identity, so "authorized" simply means at least one feed
    /// is configured.
    pub async fn status(&self) -> Result<serde_json::Value, ConnectorError> {
        self.db
            .connector_update_connection(
                CONNECTOR_ID,
                KEY,
                ConnectorConnectionUpdate::default(),
            )
            .await?;
        let row = self
            .db
            .connector_get_connection(CONNECTOR_ID, KEY)
            .await?
            .ok_or_else(|| ConnectorError::new("storage_error", "连接行初始化失败", 500))?;
        let scope = self
            .db
            .connector_get_scope(CONNECTOR_ID, KEY)
            .await?
            .map(|(json, _)| Self::scope_from_json(&json))
            .unwrap_or_default();
        let imported = self
            .db
            .connector_imported_object_count(CONNECTOR_ID)
            .await?;
        Ok(json!({
            "connector": CONNECTOR_ID,
            "key": KEY,
            "auth_status": if scope.is_empty() { "disconnected" } else { "authorized" },
            "sync_status": row.sync_status,
            "scope": scope,
            "scope_revision": row.scope_revision,
            "last_sync_at": row.last_sync_at,
            "last_success_at": row.last_success_at,
            "imported_objects": imported.max(0) as u64,
            "last_error_code": row.last_error_code,
            "last_error_message": row.last_error_message,
        }))
    }

    /// Validate and persist the feed list. Returns the new scope revision.
    pub async fn save_scope(
        &self,
        scope: &RssScope,
    ) -> Result<i64, ConnectorError> {
        for url in &scope.feed_urls {
            let parsed = url::Url::parse(url).map_err(|_| {
                ConnectorError::bad_request(format!("无效的订阅地址: {url}"))
            })?;
            if parsed.scheme() != "http" && parsed.scheme() != "https" {
                return Err(ConnectorError::bad_request(format!(
                    "订阅地址必须是 http(s): {url}"
                )));
            }
        }
        let scope_json = serde_json::to_string(scope)
            .map_err(|e| ConnectorError::new("scope_invalid", e.to_string(), 400))?;
        let revision = self
            .db
            .connector_save_scope(CONNECTOR_ID, KEY, &scope_json)
            .await?;
        self.db
            .connector_update_connection(
                CONNECTOR_ID,
                KEY,
                ConnectorConnectionUpdate {
                    auth_status: Some(
                        if scope.is_empty() { "disconnected" } else { "authorized" }.to_string(),
                    ),
                    ..Default::default()
                },
            )
            .await?;
        Ok(revision)
    }

    /// Kick one sync run: fetch every configured feed, upsert new items.
    /// Mirrors the office sync state machine (revision guard, running guard,
    /// spawned task, status finalization) on the shared connector tables.
    pub async fn start_sync(&self, expected_revision: i64) -> Result<i64, ConnectorError> {
        let row = self
            .db
            .connector_get_connection(CONNECTOR_ID, KEY)
            .await?
            .ok_or_else(|| ConnectorError::bad_request("连接不存在"))?;
        if row.scope_revision != expected_revision {
            return Err(ConnectorError::conflict("范围已变化，请刷新后重试"));
        }
        if row.sync_status == "running" || row.sync_status == "queued" {
            return Err(ConnectorError::new("rate_limited", "同步正在进行中", 429));
        }
        let scope = self
            .db
            .connector_get_scope(CONNECTOR_ID, KEY)
            .await?
            .map(|(json, _)| Self::scope_from_json(&json))
            .unwrap_or_default();
        if scope.is_empty() {
            return Err(ConnectorError::bad_request("请先添加订阅地址"));
        }
        let run_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        self.db
            .connector_update_connection(
                CONNECTOR_ID,
                KEY,
                ConnectorConnectionUpdate {
                    sync_status: Some("running".to_string()),
                    ..Default::default()
                },
            )
            .await?;
        let service = RssService {
            db: self.db.clone(),
            cancel: self.cancel.child_token(),
        };
        tokio::spawn(async move {
            let summary = run_sync(&service, &scope).await;
            let (sync_status, error_code, error_message) = match &summary {
                Ok(partial) => (
                    if *partial { "partial" } else { "idle" }.to_string(),
                    None,
                    None,
                ),
                Err(e) => (
                    "failed".to_string(),
                    Some(e.code.clone()),
                    Some(e.message.clone()),
                ),
            };
            let now = now_iso();
            let _ = service
                .db
                .connector_update_connection(
                    CONNECTOR_ID,
                    KEY,
                    ConnectorConnectionUpdate {
                        sync_status: Some(sync_status),
                        last_sync_at: Some(now.clone()),
                        last_success_at: if summary.is_ok() { Some(now) } else { None },
                        last_error_code: error_code,
                        last_error_message: error_message,
                        clear_errors: summary.is_ok(),
                        ..Default::default()
                    },
                )
                .await;
        });
        Ok(run_id)
    }

    /// Stop an in-flight sync (cancel token). `retry` re-kicks with the
    /// stored scope revision.
    pub async fn control(&self, action: &str) -> Result<(), ConnectorError> {
        match action {
            "cancel" | "pause" => {
                self.cancel.cancel();
                self.db
                    .connector_update_connection(
                        CONNECTOR_ID,
                        KEY,
                        ConnectorConnectionUpdate {
                            sync_status: Some("paused".to_string()),
                            ..Default::default()
                        },
                    )
                    .await?;
            }
            "retry" => {
                let revision = self
                    .db
                    .connector_get_connection(CONNECTOR_ID, KEY)
                    .await?
                    .map(|r| r.scope_revision)
                    .unwrap_or(0);
                self.start_sync(revision).await?;
            }
            other => {
                return Err(ConnectorError::bad_request(format!(
                    "未知操作 {other}（支持 cancel/pause/retry）"
                )))
            }
        }
        Ok(())
    }

    /// Disconnect: `erase` deletes imported content (with re-import
    /// suppression), the default retains it disabled.
    pub async fn disconnect(&self, local_data: &str) -> Result<(), ConnectorError> {
        if local_data == "erase" {
            self.db.connector_erase(CONNECTOR_ID).await?;
        } else {
            self.db.connector_disable_all_objects(CONNECTOR_ID).await?;
        }
        self.db
            .connector_clear_cursors(CONNECTOR_ID)
            .await?;
        self.db
            .connector_update_connection(
                CONNECTOR_ID,
                KEY,
                ConnectorConnectionUpdate {
                    auth_status: Some("disconnected".to_string()),
                    sync_status: Some("idle".to_string()),
                    enabled: Some(false),
                    auto_sync: Some(false),
                    bump_connection_revision: true,
                    clear_errors: true,
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }

    /// Search imported items via the shared FTS index.
    pub async fn search(&self, query: &str, limit: u32) -> Result<serde_json::Value, ConnectorError> {
        let rows = self.db.connector_search(CONNECTOR_ID, query, limit).await?;
        Ok(json!({ "items": rows }))
    }
}

/// Fetch + persist every feed. Returns true when some feed failed (partial).
async fn run_sync(service: &RssService, scope: &RssScope) -> Result<bool, ConnectorError> {
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|e| ConnectorError::new("http_error", e.to_string(), 500).transient())?;
    let mut failures = 0usize;
    for url in &scope.feed_urls {
        if service.cancel.is_cancelled() {
            break;
        }
        match fetch_feed(&client, url).await {
            Ok(items) => {
                let fetched_at = Utc::now();
                for item in items {
                    if service.cancel.is_cancelled() {
                        break;
                    }
                    let _ = service
                        .db
                        .connector_upsert_object(
                            &(ConnectorObjectDraft {
                                connector: CONNECTOR_ID.to_string(),
                                namespace: url.clone(),
                                object_kind: "item".to_string(),
                                object_id: item.id,
                                revision: None,
                                title: item.title,
                                body_text: item.body,
                                event_at: item.published,
                                fetched_at,
                                source_url: item.link,
                            }),
                        )
                        .await?;
                    // Cursor: newest item seen per feed (observability; item
                    // dedup relies on object identity).
                    let _ = service
                        .db
                        .connector_set_cursor(
                            CONNECTOR_ID,
                            KEY,
                            &format!("feed:{}", url),
                            &item.cursor,
                        )
                        .await;
                }
            }
            Err(e) => {
                tracing::warn!("rss connector: feed {url} failed: {}", e.message);
                failures += 1;
            }
        }
    }
    Ok(failures > 0)
}

struct FeedItem {
    id: String,
    cursor: String,
    title: Option<String>,
    body: String,
    link: Option<String>,
    published: Option<DateTime<Utc>>,
}

async fn fetch_feed(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<FeedItem>, ConnectorError> {
    let response = client
        .get(url)
        .header("accept", "application/rss+xml, application/atom+xml, application/xml, text/xml, */*")
        .send()
        .await
        .map_err(|e| ConnectorError::new("fetch_failed", format!("{url}: {e}"), 502).transient())?;
    if !response.status().is_success() {
        return Err(ConnectorError::new(
            "fetch_failed",
            format!("{url}: HTTP {}", response.status()),
            502,
        )
        .transient());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| ConnectorError::new("fetch_failed", format!("{url}: {e}"), 502).transient())?;
    parse_feed(url, &bytes)
}

fn parse_feed(url: &str, bytes: &[u8]) -> Result<Vec<FeedItem>, ConnectorError> {
    let feed = feed_rs::parser::parse(bytes)
        .map_err(|e| ConnectorError::new("feed_invalid", format!("{url}: {e}"), 400))?;
    let mut out = Vec::new();
    for item in feed.entries {
        let link = item
            .links
            .first()
            .map(|l| l.href.clone());
        // feed-rs initialises `id` when the feed omits it (hash of the link
        // or a UUID), so identity is always stable.
        let id = item.id.clone();
        let cursor = id.clone();
        let published = item.published.or(item.updated);
        let title = item.title.map(|t| t.content);
        let body = match item.content.as_ref().and_then(|c| c.body.clone()) {
            Some(html) => strip_html(html),
            None => item
                .summary
                .as_ref()
                .map(|d| strip_html(d.content.clone()))
                .unwrap_or_default(),
        };
        out.push(FeedItem {
            id,
            cursor,
            title,
            body,
            link,
            published,
        });
    }
    Ok(out)
}

/// Naive tag stripper for HTML feed bodies — enough to keep markup noise out
/// of the FTS index without pulling a full HTML parser.
fn strip_html(html: String) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Trait adapter — the generic REST surface talks to the channel through
/// `Connector`; RSS has a single sub-key.
#[async_trait::async_trait]
impl super::Connector for RssService {
    fn id(&self) -> &'static str {
        CONNECTOR_ID
    }

    fn keys(&self) -> Vec<String> {
        vec![KEY.to_string()]
    }

    async fn status(&self, _key: &str) -> Result<serde_json::Value, ConnectorError> {
        RssService::status(self).await
    }

    async fn refresh(&self, _key: &str) -> Result<serde_json::Value, ConnectorError> {
        RssService::status(self).await
    }

    async fn save_scope(
        &self,
        _key: &str,
        scope: &serde_json::Value,
    ) -> Result<i64, ConnectorError> {
        let parsed: RssScope = serde_json::from_value(scope.clone())
            .map_err(|e| ConnectorError::bad_request(format!("范围格式无效: {e}")))?;
        RssService::save_scope(self, &parsed).await
    }

    async fn start_sync(&self, _key: &str, expected_revision: i64) -> Result<i64, ConnectorError> {
        RssService::start_sync(self, expected_revision).await
    }

    async fn control(&self, _key: &str, action: &str) -> Result<(), ConnectorError> {
        RssService::control(self, action).await
    }

    async fn disconnect(&self, _key: &str, local_data: &str) -> Result<(), ConnectorError> {
        RssService::disconnect(self, local_data).await
    }

    async fn search(&self, query: &str, limit: u32) -> Result<serde_json::Value, ConnectorError> {
        RssService::search(self, query, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATOM_FIXTURE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>测试订阅</title>
  <entry>
    <id>urn:uuid:aaa-111</id>
    <title>第一条：中文全文检索</title>
    <link href="https://example.com/1"/>
    <updated>2026-09-17T00:00:00Z</updated>
    <content type="html">&lt;p&gt;这是 &lt;b&gt;正文&lt;/b&gt; 内容，包含标签。&lt;/p&gt;</content>
  </entry>
  <entry>
    <title>无 id 无链接的条目</title>
    <updated>2026-09-17T01:00:00Z</updated>
    <summary>纯摘要文本</summary>
  </entry>
</feed>"#;

    #[test]
    fn parse_maps_identity_and_strips_html() {
        let items = parse_feed("https://example.com/feed", ATOM_FIXTURE.as_bytes()).unwrap();
        assert_eq!(items.len(), 2);

        // Entry 1: guid wins as identity; HTML tags stripped from body.
        assert_eq!(items[0].id, "urn:uuid:aaa-111");
        assert_eq!(items[0].cursor, "urn:uuid:aaa-111");
        assert_eq!(items[0].title.as_deref(), Some("第一条：中文全文检索"));
        assert_eq!(items[0].body, "这是 正文 内容，包含标签。");
        assert_eq!(items[0].link.as_deref(), Some("https://example.com/1"));
        assert_eq!(
            items[0].published.map(|d| d.to_rfc3339()),
            Some("2026-09-17T00:00:00+00:00".to_string())
        );

        // Entry 2: no id and no link — identity falls back to a content hash
        // (stable across syncs so upsert dedup holds).
        assert!(!items[1].id.is_empty());
        assert_eq!(items[1].body, "纯摘要文本");
    }

    #[test]
    fn parse_rejects_non_feed_bytes() {
        assert!(parse_feed("https://example.com/x", b"<html><body>not a feed</body></html>").is_err());
    }

    #[test]
    fn scope_rejects_non_http_urls() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let db = screenpipe_db::DatabaseManager::new("sqlite::memory:", Default::default())
                .await
                .unwrap();
            let service = RssService::new(Arc::new(db));
            let err = service
                .save_scope(&RssScope {
                    feed_urls: vec!["ftp://example.com/feed".to_string()],
                    auto_sync: false,
                })
                .await
                .unwrap_err();
            assert_eq!(err.http, 400);
        });
    }
}
