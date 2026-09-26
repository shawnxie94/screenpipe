// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Local-first WeRead connector using the WeRead Skill Agent API Gateway.
//! Credentials are stored only in SecretStore; connector scopes/status never
//! contain the API key. Imported objects are personal bookshelf metadata,
//! highlights and notes (never book body text).

use std::{collections::HashSet, future::Future, sync::Arc, time::Duration};

use dashmap::DashMap;
use once_cell::sync::Lazy;

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use screenpipe_db::{
    ConnectorConnectionUpdate, ConnectorObjectDraft, ConnectorUpsertOutcome, DatabaseManager,
};
use screenpipe_secrets::SecretStore;

use super::ConnectorError;

pub const CONNECTOR_ID: &str = "weread";
const KEY: &str = "weread";
const SECRET_KEY: &str = "connector:weread:api-key";
const GATEWAY_URL: &str = "https://i.weread.qq.com/api/agent/gateway";
const SKILL_VERSION: &str = "1.0.4";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_NOTEBOOK_PAGES: usize = 100;
const MAX_REVIEW_PAGES: usize = 100;
static ACTIVE_RUNS: Lazy<DashMap<String, CancellationToken>> = Lazy::new(DashMap::new);
static SYNC_LOCK: Lazy<Arc<tokio::sync::Mutex<()>>> =
    Lazy::new(|| Arc::new(tokio::sync::Mutex::new(())));

fn try_sync_guard() -> Result<tokio::sync::OwnedMutexGuard<()>, ConnectorError> {
    SYNC_LOCK
        .clone()
        .try_lock_owned()
        .map_err(|_| ConnectorError::new("rate_limited", "微信读书同步正在进行中", 429))
}

fn cancel_active_sync() {
    if let Some(active) = ACTIVE_RUNS.get(KEY) {
        active.cancel();
    }
}

async fn cancellable_request<T>(
    cancel: &CancellationToken,
    request: impl Future<Output = Result<T, ConnectorError>>,
) -> Result<Option<T>, ConnectorError> {
    tokio::select! {
        _ = cancel.cancelled() => Ok(None),
        result = request => result.map(Some),
    }
}

struct ActiveRunRegistration;

impl Drop for ActiveRunRegistration {
    fn drop(&mut self) {
        ACTIVE_RUNS.remove(KEY);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WeReadScope {
    #[serde(default)]
    pub auto_sync: bool,
}

#[derive(Clone)]
struct WeReadClient {
    http: reqwest::Client,
    api_key: String,
    gateway_url: String,
}

impl WeReadClient {
    fn new(api_key: String) -> Result<Self, ConnectorError> {
        Self::with_gateway(api_key, GATEWAY_URL.to_string())
    }

    fn with_gateway(api_key: String, gateway_url: String) -> Result<Self, ConnectorError> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| ConnectorError::new("http_error", "无法初始化微信读书连接", 500))?;
        Ok(Self {
            http,
            api_key,
            gateway_url,
        })
    }

    async fn call(&self, api_name: &str, params: Value) -> Result<Value, ConnectorError> {
        let mut body = params.as_object().cloned().unwrap_or_default();
        body.insert("api_name".to_string(), Value::String(api_name.to_string()));
        body.insert(
            "skill_version".to_string(),
            Value::String(SKILL_VERSION.to_string()),
        );
        let mut response = self
            .http
            .post(&self.gateway_url)
            .bearer_auth(&self.api_key)
            .json(&Value::Object(body))
            .send()
            .await
            .map_err(|_| {
                ConnectorError::new("upstream_unavailable", "微信读书服务暂时不可用", 502)
                    .transient()
            })?;
        if !response.status().is_success() {
            return Err(ConnectorError::new(
                "upstream_http_error",
                "微信读书服务返回错误，请检查连接后重试",
                502,
            )
            .transient());
        }
        if response
            .content_length()
            .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
        {
            return Err(ConnectorError::new(
                "upstream_response_too_large",
                "微信读书响应超出大小限制",
                502,
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            ConnectorError::new("upstream_unavailable", "读取微信读书响应失败", 502).transient()
        })? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(ConnectorError::new(
                    "upstream_response_too_large",
                    "微信读书响应超出大小限制",
                    502,
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            ConnectorError::new("upstream_invalid_response", "微信读书返回了无效响应", 502)
        })?;
        check_gateway_error(&value)?;
        Ok(unwrap_gateway_data(value))
    }

    async fn validate(&self) -> Result<(), ConnectorError> {
        self.call("/shelf/sync", json!({})).await.map(|_| ())
    }

    async fn bookshelf(&self) -> Result<Value, ConnectorError> {
        self.call("/shelf/sync", json!({})).await
    }

    async fn notebook_page(&self, last_sort: Option<i64>) -> Result<Value, ConnectorError> {
        let mut params = json!({ "count": 100 });
        if let Some(last_sort) = last_sort {
            params["lastSort"] = json!(last_sort);
        }
        self.call("/user/notebooks", params).await
    }

    async fn highlights(&self, book_id: &str) -> Result<Value, ConnectorError> {
        self.call("/book/bookmarklist", json!({ "bookId": book_id }))
            .await
    }

    async fn review_page(&self, book_id: &str, synckey: i64) -> Result<Value, ConnectorError> {
        self.call(
            "/review/list/mine",
            json!({ "bookid": book_id, "synckey": synckey, "count": 100 }),
        )
        .await
    }
}

fn unwrap_gateway_data(value: Value) -> Value {
    // The Skill gateway currently returns the endpoint fields directly. Keep
    // compatibility with wrappers used by older gateway deployments.
    value
        .get("data")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or(value)
}

fn check_gateway_error(value: &Value) -> Result<(), ConnectorError> {
    if value.get("upgrade_info").is_some() {
        return Err(ConnectorError::new(
            "skill_version_update_required",
            "微信读书 Skill 版本需要更新，请更新 screenpipe 后重试",
            502,
        ));
    }
    let code = value.get("errcode").and_then(Value::as_i64).unwrap_or(0);
    if code != 0 {
        let (code, message, http) = match code {
            401 | 403 => ("credential_invalid", "微信读书 API Key 无效或已过期", 401),
            _ => ("upstream_rejected", "微信读书未能处理请求，请稍后重试", 502),
        };
        return Err(ConnectorError::new(code, message, http));
    }
    Ok(())
}

fn content_hash(text: &str) -> Option<String> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then(|| hex::encode(Sha256::digest(normalized.as_bytes())))
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|v| match v {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
    })
}

fn unix_time(value: &Value, keys: &[&str]) -> Option<DateTime<Utc>> {
    let seconds = keys
        .iter()
        .find_map(|key| value.get(*key).and_then(Value::as_i64))?;
    Utc.timestamp_opt(seconds, 0).single()
}

fn array_field<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn has_more(value: &Value) -> bool {
    value
        .get("hasMore")
        .is_some_and(|v| v.as_bool().unwrap_or_else(|| v.as_i64().unwrap_or(0) != 0))
}

fn pagination_limit_reached(has_more: bool, pages_fetched: usize, max_pages: usize) -> bool {
    has_more && pages_fetched >= max_pages
}

fn gateway_nested_book(value: &Value) -> &Value {
    value.get("book").filter(|v| v.is_object()).unwrap_or(value)
}

fn book_id(value: &Value) -> Option<String> {
    string_field(value, &["bookId", "bookid", "id"])
        .or_else(|| string_field(gateway_nested_book(value), &["bookId", "bookid", "id"]))
}

pub struct WeReadService {
    pub db: Arc<DatabaseManager>,
    secret_store: Option<Arc<SecretStore>>,
}

impl WeReadService {
    pub fn new(db: Arc<DatabaseManager>, secret_store: Option<Arc<SecretStore>>) -> Self {
        Self { db, secret_store }
    }

    fn scope_from_json(raw: &str) -> WeReadScope {
        serde_json::from_str(raw).unwrap_or_default()
    }

    async fn api_key(&self) -> Result<String, ConnectorError> {
        let store = self.secret_store.as_ref().ok_or_else(|| {
            ConnectorError::new("secret_store_unavailable", "本机凭证存储不可用", 503)
        })?;
        let bytes = store
            .get(SECRET_KEY)
            .await
            .map_err(|_| {
                ConnectorError::new("secret_store_error", "无法读取微信读书连接凭证", 500)
            })?
            .ok_or_else(|| {
                ConnectorError::new("not_configured", "请先配置微信读书 API Key", 400)
            })?;
        String::from_utf8(bytes).map_err(|_| {
            ConnectorError::new(
                "secret_store_error",
                "微信读书连接凭证格式无效，请重新配置",
                500,
            )
        })
    }

    async fn api_client(&self) -> Result<WeReadClient, ConnectorError> {
        WeReadClient::new(self.api_key().await?)
    }

    pub async fn status(&self) -> Result<Value, ConnectorError> {
        self.db
            .connector_update_connection(CONNECTOR_ID, KEY, ConnectorConnectionUpdate::default())
            .await?;
        let mut row = self
            .db
            .connector_get_connection(CONNECTOR_ID, KEY)
            .await?
            .ok_or_else(|| ConnectorError::new("storage_error", "连接状态初始化失败", 500))?;
        let mut last_run = self
            .db
            .connector_sync_runs_recent(CONNECTOR_ID, KEY, 1)
            .await?
            .into_iter()
            .next();
        let has_open_run = last_run
            .as_ref()
            .map(|run| run.finished_at.is_none())
            .unwrap_or(false);
        if (matches!(row.sync_status.as_str(), "running" | "queued") || has_open_run)
            && !ACTIVE_RUNS.contains_key(KEY)
        {
            if let Ok(_sync_guard) = SYNC_LOCK.clone().try_lock_owned() {
                if !ACTIVE_RUNS.contains_key(KEY) {
                    self.recover_stale_sync().await?;
                    row = self
                        .db
                        .connector_get_connection(CONNECTOR_ID, KEY)
                        .await?
                        .ok_or_else(|| {
                            ConnectorError::new("storage_error", "连接状态初始化失败", 500)
                        })?;
                    last_run = self
                        .db
                        .connector_sync_runs_recent(CONNECTOR_ID, KEY, 1)
                        .await?
                        .into_iter()
                        .next();
                }
            }
        }
        let scope = self
            .db
            .connector_get_scope(CONNECTOR_ID, KEY)
            .await?
            .map(|(raw, _)| Self::scope_from_json(&raw))
            .unwrap_or_default();
        let configured = match &self.secret_store {
            Some(store) => store
                .get(SECRET_KEY)
                .await
                .map_err(|_| {
                    ConnectorError::new("secret_store_error", "无法读取微信读书连接状态", 500)
                })?
                .is_some(),
            None => false,
        };
        let imported = self
            .db
            .connector_imported_object_count(CONNECTOR_ID)
            .await?;
        Ok(json!({
            "connector": CONNECTOR_ID,
            "key": KEY,
            "auth_status": if configured { "authorized" } else { "disconnected" },
            "credential_configured": configured,
            "sync_status": row.sync_status,
            "scope": scope,
            "scope_revision": row.scope_revision,
            "last_sync_at": row.last_sync_at,
            "last_success_at": row.last_success_at,
            "imported_objects": imported.max(0) as u64,
            "last_error_code": row.last_error_code,
            "last_error_message": row.last_error_message,
            "last_run": last_run,
        }))
    }

    pub async fn configure_key(&self, api_key: &str) -> Result<(), ConnectorError> {
        let key = api_key.trim();
        if !key.starts_with("wrk-") || key.len() < 8 || key.len() > 2048 {
            return Err(ConnectorError::bad_request("微信读书 API Key 格式无效"));
        }
        let store = self.secret_store.as_ref().ok_or_else(|| {
            ConnectorError::new(
                "secret_store_unavailable",
                "本机凭证存储不可用，无法保存 API Key",
                503,
            )
        })?;
        let client = WeReadClient::new(key.to_string())?;
        // Validate against the user's bookshelf before storing; neither the key
        // nor upstream response body is included in any error or log.
        client.validate().await?;
        store.set(SECRET_KEY, key.as_bytes()).await.map_err(|_| {
            ConnectorError::new("secret_store_error", "无法安全保存微信读书 API Key", 500)
        })?;
        self.db
            .connector_update_connection(
                CONNECTOR_ID,
                KEY,
                ConnectorConnectionUpdate {
                    auth_status: Some("authorized".to_string()),
                    enabled: Some(true),
                    bump_connection_revision: true,
                    clear_errors: true,
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }

    pub async fn remove_key(&self) -> Result<(), ConnectorError> {
        cancel_active_sync();
        let _sync_guard = SYNC_LOCK.clone().lock_owned().await;
        self.remove_key_without_sync_lock().await
    }

    async fn remove_key_without_sync_lock(&self) -> Result<(), ConnectorError> {
        if let Some(store) = &self.secret_store {
            store.delete(SECRET_KEY).await.map_err(|_| {
                ConnectorError::new("secret_store_error", "无法删除微信读书连接凭证", 500)
            })?;
        }
        self.db
            .connector_update_connection(
                CONNECTOR_ID,
                KEY,
                ConnectorConnectionUpdate {
                    auth_status: Some("disconnected".to_string()),
                    enabled: Some(false),
                    auto_sync: Some(false),
                    bump_connection_revision: true,
                    clear_errors: true,
                    sync_status: Some("idle".to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }

    pub async fn save_scope(&self, scope: &WeReadScope) -> Result<i64, ConnectorError> {
        let json = serde_json::to_string(scope)
            .map_err(|_| ConnectorError::new("scope_invalid", "同步设置无效", 400))?;
        let configured = match &self.secret_store {
            Some(store) => store
                .get(SECRET_KEY)
                .await
                .map_err(|_| {
                    ConnectorError::new("secret_store_error", "无法读取微信读书连接状态", 500)
                })?
                .is_some(),
            None => false,
        };
        let revision = self
            .db
            .connector_save_scope(CONNECTOR_ID, KEY, &json)
            .await?;
        self.db
            .connector_update_connection(
                CONNECTOR_ID,
                KEY,
                ConnectorConnectionUpdate {
                    auto_sync: Some(scope.auto_sync),
                    auth_status: Some(
                        if configured {
                            "authorized"
                        } else {
                            "disconnected"
                        }
                        .to_string(),
                    ),
                    ..Default::default()
                },
            )
            .await?;
        Ok(revision)
    }

    pub async fn start_sync(&self, expected_revision: i64) -> Result<i64, ConnectorError> {
        // Reserve the single-run slot before the first await so concurrent starts
        // and disconnect cannot pass each other between the status check and spawn.
        let sync_guard = try_sync_guard()?;
        if ACTIVE_RUNS.contains_key(KEY) {
            return Err(ConnectorError::new(
                "rate_limited",
                "微信读书同步正在进行中",
                429,
            ));
        }
        let cancel = CancellationToken::new();
        ACTIVE_RUNS.insert(KEY.to_string(), cancel.clone());
        let active_run = ActiveRunRegistration;

        let row = self
            .db
            .connector_get_connection(CONNECTOR_ID, KEY)
            .await?
            .ok_or_else(|| ConnectorError::bad_request("微信读书连接尚未配置"))?;
        if row.scope_revision != expected_revision {
            return Err(ConnectorError::conflict("连接设置已变化，请刷新后重试"));
        }
        let has_open_run = self
            .db
            .connector_sync_runs_recent(CONNECTOR_ID, KEY, 1)
            .await?
            .first()
            .map(|run| run.finished_at.is_none())
            .unwrap_or(false);
        if row.sync_status == "running" || row.sync_status == "queued" || has_open_run {
            // We own the sync lock and no active task is registered, so this is
            // a persisted state left behind by an interrupted finalization.
            self.recover_stale_sync().await?;
        }
        let client = self.api_client().await?;
        let db = self.db.clone();
        let run_id = self.db.connector_sync_run_start(CONNECTOR_ID, KEY).await?;
        if let Err(error) = self
            .db
            .connector_update_connection(
                CONNECTOR_ID,
                KEY,
                ConnectorConnectionUpdate {
                    auth_status: Some("authorized".to_string()),
                    sync_status: Some("running".to_string()),
                    enabled: Some(true),
                    ..Default::default()
                },
            )
            .await
        {
            let _ = self
                .db
                .connector_sync_run_finish(
                    run_id,
                    "failed",
                    0,
                    0,
                    1,
                    Some("storage_error"),
                    Some("无法更新微信读书同步状态"),
                )
                .await;
            return Err(error.into());
        }
        tokio::spawn(async move {
            // Drop registration before releasing the lock, so a new start can
            // never observe an unlocked slot with stale active-run state.
            let _sync_guard = sync_guard;
            let _active_run = active_run;
            let result = run_sync(&db, &client, &cancel, run_id).await;
            let now = Utc::now().to_rfc3339();
            let (status, code, message, successful, processed, skipped, failed) = match result {
                Ok(summary) => (
                    if summary.cancelled {
                        "cancelled"
                    } else if summary.failures > 0 {
                        "partial"
                    } else {
                        "succeeded"
                    },
                    None,
                    None,
                    true,
                    summary.processed,
                    summary.skipped,
                    summary.failures,
                ),
                Err(error) => (
                    "failed",
                    Some(error.code.clone()),
                    Some(error.message.clone()),
                    false,
                    0,
                    0,
                    1,
                ),
            };
            if let Err(error) = db
                .connector_update_connection(
                    CONNECTOR_ID,
                    KEY,
                    ConnectorConnectionUpdate {
                        sync_status: Some(
                            if status == "succeeded" {
                                "idle"
                            } else {
                                status
                            }
                            .to_string(),
                        ),
                        last_sync_at: Some(now.clone()),
                        last_success_at: if successful { Some(now) } else { None },
                        last_error_code: code.clone(),
                        last_error_message: message.clone(),
                        clear_errors: successful,
                        ..Default::default()
                    },
                )
                .await
            {
                tracing::warn!(run_id, %error, "weread connector: failed to persist sync status; a later status check will recover it");
            }
            if let Err(error) = db
                .connector_sync_run_finish(
                    run_id,
                    status,
                    processed,
                    skipped,
                    failed,
                    code.as_deref(),
                    message.as_deref(),
                )
                .await
            {
                tracing::warn!(run_id, %error, "weread connector: failed to persist run history; a later status check will recover it");
            }
        });
        Ok(run_id)
    }

    pub async fn control(&self, action: &str) -> Result<(), ConnectorError> {
        match action {
            "cancel" | "pause" => {
                if let Some(active) = ACTIVE_RUNS.get(KEY) {
                    active.cancel();
                }
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
                Ok(())
            }
            "retry" => {
                let revision = self
                    .db
                    .connector_get_connection(CONNECTOR_ID, KEY)
                    .await?
                    .map(|r| r.scope_revision)
                    .unwrap_or(0);
                self.start_sync(revision).await.map(|_| ())
            }
            other => Err(ConnectorError::bad_request(format!("未知操作 {other}"))),
        }
    }

    pub async fn disconnect(&self, local_data: &str) -> Result<(), ConnectorError> {
        cancel_active_sync();
        // Keep the sync slot until the credential and local-data policy are
        // applied. New starts are rejected while this lock is held.
        let _sync_guard = SYNC_LOCK.clone().lock_owned().await;
        self.remove_key_without_sync_lock().await?;
        if local_data == "erase" {
            self.db.connector_erase(CONNECTOR_ID).await?;
        } else {
            self.db.connector_disable_all_objects(CONNECTOR_ID).await?;
        }
        self.db.connector_clear_cursors(CONNECTOR_ID).await?;
        Ok(())
    }

    async fn recover_stale_sync(&self) -> Result<(), ConnectorError> {
        let connection = self
            .db
            .connector_get_connection(CONNECTOR_ID, KEY)
            .await?
            .ok_or_else(|| ConnectorError::new("storage_error", "连接状态初始化失败", 500))?;
        if let Some(run) = self
            .db
            .connector_sync_runs_recent(CONNECTOR_ID, KEY, 1)
            .await?
            .into_iter()
            .next()
            .filter(|run| run.finished_at.is_none())
        {
            let terminal_write_matches_run = connection
                .last_sync_at
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .zip(DateTime::parse_from_rfc3339(&run.started_at).ok())
                .map(|(finished_at, started_at)| finished_at >= started_at)
                .unwrap_or(false);
            let recovered_status = if terminal_write_matches_run {
                match connection.sync_status.as_str() {
                    "idle" => Some("succeeded"),
                    "partial" => Some("partial"),
                    "cancelled" => Some("cancelled"),
                    "failed" => Some("failed"),
                    _ => None,
                }
            } else {
                None
            };
            let (status, error_code, error_message, failed) = match recovered_status {
                Some(status) => (
                    status,
                    connection.last_error_code.as_deref(),
                    connection.last_error_message.as_deref(),
                    if matches!(status, "partial" | "failed") {
                        run.failed.max(1)
                    } else {
                        run.failed
                    },
                ),
                None => (
                    "failed",
                    Some("interrupted"),
                    Some("同步任务已中断，连接状态已恢复"),
                    run.failed.max(1),
                ),
            };
            self.db
                .connector_sync_run_finish(
                    run.id,
                    status,
                    run.processed,
                    run.skipped,
                    failed,
                    error_code,
                    error_message,
                )
                .await?;
        }
        if matches!(connection.sync_status.as_str(), "running" | "queued") {
            self.db
                .connector_update_connection(
                    CONNECTOR_ID,
                    KEY,
                    ConnectorConnectionUpdate {
                        sync_status: Some("idle".to_string()),
                        ..Default::default()
                    },
                )
                .await?;
        }
        Ok(())
    }

    pub async fn search(&self, query: &str, limit: u32) -> Result<Value, ConnectorError> {
        let rows = self.db.connector_search(CONNECTOR_ID, query, limit).await?;
        Ok(json!({ "items": rows }))
    }
}

#[derive(Default)]
struct SyncSummary {
    failures: i64,
    processed: i64,
    skipped: i64,
    cancelled: bool,
}

async fn run_sync(
    db: &DatabaseManager,
    client: &WeReadClient,
    cancel: &CancellationToken,
    run_id: i64,
) -> Result<SyncSummary, ConnectorError> {
    let fetched_at = Utc::now();
    let mut summary = SyncSummary::default();
    if cancel.is_cancelled() {
        summary.cancelled = true;
        return Ok(summary);
    }
    let Some(shelf) = cancellable_request(cancel, client.bookshelf()).await? else {
        summary.cancelled = true;
        return Ok(summary);
    };
    let books = array_field(&shelf, "books");
    for value in books {
        if cancel.is_cancelled() {
            summary.cancelled = true;
            break;
        }
        let Some(id) = book_id(value) else { continue };
        let book = gateway_nested_book(value);
        let title =
            string_field(book, &["title", "name"]).unwrap_or_else(|| format!("微信读书 {id}"));
        let author = string_field(book, &["author", "authorName"]).unwrap_or_default();
        let category = string_field(book, &["category"]).unwrap_or_default();
        let metadata = json!({"book_id":id,"title":title,"author":author,"category":category,"reading_progress":book.get("readingProgress").or_else(|| book.get("progress")),"finished":book.get("finishReading"),"last_read":book.get("readUpdateTime"),"deep_link":book.get("deepLink")});
        let body = format!("{title}\n{author}\n{category}");
        let outcome = upsert(
            db,
            "book",
            &id,
            Some(&title),
            &body,
            Some(metadata),
            None,
            fetched_at,
        )
        .await?;
        count_outcome(&mut summary, outcome);
    }
    let albums = array_field(&shelf, "albums");
    for value in albums {
        if cancel.is_cancelled() {
            summary.cancelled = true;
            break;
        }
        let info = value.get("albumInfo").unwrap_or(value);
        let Some(id) = string_field(info, &["albumId", "id"]) else {
            continue;
        };
        let title =
            string_field(info, &["name", "title"]).unwrap_or_else(|| format!("微信读书专辑 {id}"));
        let author = string_field(info, &["authorName", "author"]).unwrap_or_default();
        let body = format!("{title}\n{author}");
        let outcome = upsert(
            db,
            "album",
            &id,
            Some(&title),
            &body,
            Some(info.clone()),
            None,
            fetched_at,
        )
        .await?;
        count_outcome(&mut summary, outcome);
    }

    let mut last_sort = None;
    let mut seen_books = HashSet::new();
    for page_index in 0..MAX_NOTEBOOK_PAGES {
        if cancel.is_cancelled() {
            summary.cancelled = true;
            break;
        }
        let Some(page) = cancellable_request(cancel, client.notebook_page(last_sort)).await? else {
            summary.cancelled = true;
            break;
        };
        let notebooks = array_field(&page, "books");
        if notebooks.is_empty() {
            break;
        }
        for notebook in notebooks {
            if cancel.is_cancelled() {
                summary.cancelled = true;
                break;
            }
            let Some(id) = book_id(notebook) else {
                continue;
            };
            if !seen_books.insert(id.clone()) {
                continue;
            }
            let title = string_field(gateway_nested_book(notebook), &["title", "name"])
                .unwrap_or_else(|| format!("微信读书 {id}"));
            match sync_book_notes(db, client, cancel, &id, &title, fetched_at, &mut summary).await {
                Ok(()) => {}
                Err(error) => {
                    summary.failures += 1;
                    tracing::warn!(code = %error.code, "weread connector: a book's notes could not be imported");
                }
            }
        }
        let more = has_more(&page);
        if !more {
            break;
        }
        if pagination_limit_reached(more, page_index + 1, MAX_NOTEBOOK_PAGES) {
            return Err(ConnectorError::new(
                "pagination_limit_reached",
                "微信读书笔记列表超过同步分页上限，当前同步不完整；请稍后重试或联系支持",
                502,
            ));
        }
        last_sort = notebooks
            .last()
            .and_then(|v| v.get("sort"))
            .and_then(Value::as_i64);
        if last_sort.is_none() {
            return Err(ConnectorError::new(
                "pagination_invalid",
                "微信读书笔记分页游标无效，已停止同步以避免重复读取",
                502,
            ));
        }
    }
    let _ = run_id;
    Ok(summary)
}

async fn sync_book_notes(
    db: &DatabaseManager,
    client: &WeReadClient,
    cancel: &CancellationToken,
    book_id: &str,
    title: &str,
    fetched_at: DateTime<Utc>,
    summary: &mut SyncSummary,
) -> Result<(), ConnectorError> {
    let Some(highlights) = cancellable_request(cancel, client.highlights(book_id)).await? else {
        summary.cancelled = true;
        return Ok(());
    };
    for item in array_field(&highlights, "updated") {
        if cancel.is_cancelled() {
            summary.cancelled = true;
            return Ok(());
        }
        let Some(id) = string_field(item, &["bookmarkId", "id"]) else {
            continue;
        };
        let text = string_field(item, &["markText", "text"]).unwrap_or_default();
        let chapter = string_field(item, &["chapterName"])
            .or_else(|| {
                item.get("chapterUid")
                    .and_then(Value::as_i64)
                    .map(|v| v.to_string())
            })
            .unwrap_or_default();
        let body = format!("{title}\n{chapter}\n{text}");
        let event_at = unix_time(item, &["createTime"]);
        let outcome = upsert(
            db,
            "highlight",
            &format!("{book_id}:{id}"),
            Some(title),
            &body,
            Some(item.clone()),
            event_at,
            fetched_at,
        )
        .await?;
        count_outcome(summary, outcome);
    }

    let mut synckey = 0i64;
    for page_index in 0..MAX_REVIEW_PAGES {
        if cancel.is_cancelled() {
            summary.cancelled = true;
            break;
        }
        let Some(page) = cancellable_request(cancel, client.review_page(book_id, synckey)).await?
        else {
            summary.cancelled = true;
            break;
        };
        for item in array_field(&page, "reviews") {
            if cancel.is_cancelled() {
                summary.cancelled = true;
                return Ok(());
            }
            let review = item.get("review").filter(|v| v.is_object()).unwrap_or(item);
            let Some(id) = string_field(review, &["reviewId", "id"]) else {
                continue;
            };
            let content = string_field(review, &["content", "abstract"]).unwrap_or_default();
            if content.is_empty() {
                continue;
            }
            let abstract_text = string_field(review, &["abstract"]).unwrap_or_default();
            let chapter = string_field(review, &["chapterName"]).unwrap_or_default();
            let body = [
                title,
                chapter.as_str(),
                abstract_text.as_str(),
                content.as_str(),
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
            let event_at = unix_time(review, &["createTime"]);
            let outcome = upsert(
                db,
                "review",
                &format!("{book_id}:{id}"),
                Some(title),
                &body,
                Some(review.clone()),
                event_at,
                fetched_at,
            )
            .await?;
            count_outcome(summary, outcome);
        }
        let more = has_more(&page);
        if !more {
            break;
        }
        if pagination_limit_reached(more, page_index + 1, MAX_REVIEW_PAGES) {
            return Err(ConnectorError::new(
                "pagination_limit_reached",
                "微信读书想法列表超过同步分页上限，当前同步不完整；请稍后重试或联系支持",
                502,
            ));
        }
        let next = page
            .get("synckey")
            .and_then(Value::as_i64)
            .unwrap_or(synckey);
        if next == synckey {
            return Err(ConnectorError::new(
                "pagination_invalid",
                "微信读书想法分页游标未前进，已停止读取",
                502,
            ));
        }
        synckey = next;
    }
    Ok(())
}

async fn upsert(
    db: &DatabaseManager,
    kind: &str,
    id: &str,
    title: Option<&str>,
    body: &str,
    metadata: Option<Value>,
    event_at: Option<DateTime<Utc>>,
    fetched_at: DateTime<Utc>,
) -> Result<ConnectorUpsertOutcome, ConnectorError> {
    let source_url = metadata
        .as_ref()
        .and_then(|value| {
            ["deepLink", "deep_link", "url", "link"]
                .iter()
                .find_map(|key| value.get(key).and_then(Value::as_str))
        })
        .filter(|url| url.starts_with("https://weread.qq.com/"))
        .map(str::to_string)
        .or_else(|| {
            let book_id = id.split(':').next().unwrap_or(id);
            (matches!(kind, "book" | "highlight" | "review")
                && !book_id.is_empty()
                && book_id
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
            .then(|| format!("https://weread.qq.com/web/bookDetail/{book_id}"))
        });
    let body_text = match metadata {
        Some(metadata) => format!("{body}\n{}", metadata),
        None => body.to_string(),
    };
    let draft = ConnectorObjectDraft {
        connector: CONNECTOR_ID.to_string(),
        namespace: KEY.to_string(),
        object_kind: kind.to_string(),
        object_id: id.to_string(),
        revision: None,
        title: title.map(str::to_string),
        content_hash: content_hash(&body_text),
        body_text,
        event_at,
        fetched_at,
        source_url,
    };
    db.connector_upsert_object(&draft).await.map_err(Into::into)
}

fn count_outcome(summary: &mut SyncSummary, outcome: ConnectorUpsertOutcome) {
    match outcome {
        ConnectorUpsertOutcome::Created => summary.processed += 1,
        ConnectorUpsertOutcome::Unchanged | ConnectorUpsertOutcome::Duplicate => {
            summary.skipped += 1
        }
    }
}

#[async_trait::async_trait]
impl super::Connector for WeReadService {
    fn id(&self) -> &'static str {
        CONNECTOR_ID
    }
    fn keys(&self) -> Vec<String> {
        vec![KEY.to_string()]
    }
    async fn status(&self, _key: &str) -> Result<Value, ConnectorError> {
        WeReadService::status(self).await
    }
    async fn refresh(&self, _key: &str) -> Result<Value, ConnectorError> {
        WeReadService::status(self).await
    }
    async fn save_scope(&self, _key: &str, scope: &Value) -> Result<i64, ConnectorError> {
        let scope: WeReadScope = serde_json::from_value(scope.clone())
            .map_err(|_| ConnectorError::bad_request("同步设置格式无效"))?;
        WeReadService::save_scope(self, &scope).await
    }
    async fn start_sync(&self, _key: &str, expected_revision: i64) -> Result<i64, ConnectorError> {
        WeReadService::start_sync(self, expected_revision).await
    }
    async fn control(&self, _key: &str, action: &str) -> Result<(), ConnectorError> {
        WeReadService::control(self, action).await
    }
    async fn disconnect(&self, _key: &str, local_data: &str) -> Result<(), ConnectorError> {
        WeReadService::disconnect(self, local_data).await
    }
    async fn search(&self, query: &str, limit: u32) -> Result<Value, ConnectorError> {
        WeReadService::search(self, query, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{body_json, header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    static SYNC_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[test]
    fn gateway_error_messages_never_reflect_response_or_credentials() {
        let err =
            check_gateway_error(&json!({"errcode":401,"errmsg":"secret wrk-should-not-leak"}))
                .unwrap_err();
        assert_eq!(err.code, "credential_invalid");
        assert!(!err.message.contains("wrk-"));
        assert!(check_gateway_error(&json!({"upgrade_info":{"message":"update"}})).is_err());
    }

    #[tokio::test]
    async fn api_client_posts_flat_versioned_gateway_request_with_bearer_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gateway"))
            .and(header("authorization", "Bearer wrk-test-token"))
            .and(body_json(
                json!({"api_name":"/shelf/sync","skill_version":SKILL_VERSION}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"books":[]})))
            .expect(1)
            .mount(&server)
            .await;
        let client = WeReadClient::with_gateway(
            "wrk-test-token".into(),
            format!("{}/gateway", server.uri()),
        )
        .unwrap();
        let shelf = client.bookshelf().await.unwrap();
        assert_eq!(array_field(&shelf, "books").len(), 0);
    }

    #[test]
    fn notebooks_cursor_and_review_fields_follow_skill_contract() {
        assert!(has_more(&json!({"hasMore":1})));
        assert!(has_more(&json!({"hasMore":true})));
        assert!(!has_more(&json!({"hasMore":0})));
        assert!(!pagination_limit_reached(false, 100, 100));
        assert!(!pagination_limit_reached(true, 99, 100));
        assert!(pagination_limit_reached(true, 100, 100));
        assert_eq!(
            book_id(&json!({"bookId":"b-1","sort":123})).as_deref(),
            Some("b-1")
        );
        assert_eq!(
            unix_time(&json!({"createTime":1}), &["createTime"])
                .unwrap()
                .timestamp(),
            1
        );
    }

    #[test]
    fn unwraps_legacy_data_envelope_and_hashes_content_stably() {
        assert_eq!(
            unwrap_gateway_data(json!({"data":{"books":[]}})),
            json!({"books":[]})
        );
        assert_eq!(content_hash("a  b"), content_hash("a b"));
        assert_eq!(content_hash("  "), None);
    }

    #[tokio::test]
    async fn cancellation_aborts_an_in_flight_request_future() {
        let cancel = CancellationToken::new();
        let request_cancel = cancel.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let request = async move {
            let _ = started_tx.send(());
            std::future::pending::<Result<(), ConnectorError>>().await
        };
        let task = tokio::spawn(async move { cancellable_request(&request_cancel, request).await });

        started_rx.await.unwrap();
        cancel.cancel();

        assert!(task.await.unwrap().unwrap().is_none());
    }

    #[tokio::test]
    async fn status_recovers_an_interrupted_running_sync_and_closes_its_history() {
        let _test_guard = SYNC_TEST_LOCK.lock().await;
        let db = Arc::new(
            DatabaseManager::new("sqlite::memory:", Default::default())
                .await
                .unwrap(),
        );
        db.connector_update_connection(
            CONNECTOR_ID,
            KEY,
            ConnectorConnectionUpdate {
                sync_status: Some("running".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let run_id = db
            .connector_sync_run_start(CONNECTOR_ID, KEY)
            .await
            .unwrap();
        let service = WeReadService::new(db.clone(), None);

        let status = service.status().await.unwrap();
        let connection = db
            .connector_get_connection(CONNECTOR_ID, KEY)
            .await
            .unwrap()
            .unwrap();
        let run = db
            .connector_sync_runs_recent(CONNECTOR_ID, KEY, 1)
            .await
            .unwrap()
            .remove(0);

        assert_eq!(connection.sync_status, "idle");
        assert_eq!(status["sync_status"], "idle");
        assert_eq!(run.id, run_id);
        assert_eq!(run.status, "failed");
        assert_eq!(run.error_code.as_deref(), Some("interrupted"));
        assert!(run.finished_at.is_some());
    }

    #[tokio::test]
    async fn status_preserves_success_when_run_history_finish_was_lost() {
        let _test_guard = SYNC_TEST_LOCK.lock().await;
        let db = Arc::new(
            DatabaseManager::new("sqlite::memory:", Default::default())
                .await
                .unwrap(),
        );
        let run_id = db
            .connector_sync_run_start(CONNECTOR_ID, KEY)
            .await
            .unwrap();
        let completed_at = Utc::now().to_rfc3339();
        db.connector_update_connection(
            CONNECTOR_ID,
            KEY,
            ConnectorConnectionUpdate {
                sync_status: Some("idle".to_string()),
                last_sync_at: Some(completed_at.clone()),
                last_success_at: Some(completed_at),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let service = WeReadService::new(db.clone(), None);

        let status = service.status().await.unwrap();
        let run = db
            .connector_sync_runs_recent(CONNECTOR_ID, KEY, 1)
            .await
            .unwrap()
            .remove(0);

        assert_eq!(status["sync_status"], "idle");
        assert_eq!(run.id, run_id);
        assert_eq!(run.status, "succeeded");
        assert!(run.finished_at.is_some());
        assert_eq!(run.error_code, None);
    }

    #[tokio::test]
    async fn status_recovers_an_open_run_even_if_connection_status_is_idle() {
        let _test_guard = SYNC_TEST_LOCK.lock().await;
        let db = Arc::new(
            DatabaseManager::new("sqlite::memory:", Default::default())
                .await
                .unwrap(),
        );
        let run_id = db
            .connector_sync_run_start(CONNECTOR_ID, KEY)
            .await
            .unwrap();
        let service = WeReadService::new(db.clone(), None);

        let status = service.status().await.unwrap();
        let run = db
            .connector_sync_runs_recent(CONNECTOR_ID, KEY, 1)
            .await
            .unwrap()
            .remove(0);

        assert_eq!(status["sync_status"], "idle");
        assert_eq!(run.id, run_id);
        assert_eq!(run.status, "failed");
        assert_eq!(run.error_code.as_deref(), Some("interrupted"));
        assert!(run.finished_at.is_some());
    }

    #[tokio::test]
    async fn concurrent_sync_starts_are_rejected() {
        let _test_guard = SYNC_TEST_LOCK.lock().await;
        let first = try_sync_guard().unwrap();

        assert!(try_sync_guard().is_err());
        drop(first);
        assert!(try_sync_guard().is_ok());
    }

    #[tokio::test]
    async fn key_removal_cancels_and_waits_for_an_active_sync() {
        let _test_guard = SYNC_TEST_LOCK.lock().await;
        let sync_guard = try_sync_guard().unwrap();
        let db = Arc::new(
            DatabaseManager::new("sqlite::memory:", Default::default())
                .await
                .unwrap(),
        );
        let service = WeReadService::new(db, None);
        let cancel = CancellationToken::new();
        ACTIVE_RUNS.insert(KEY.to_string(), cancel.clone());

        let removal = tokio::spawn(async move { service.remove_key().await });
        tokio::task::yield_now().await;
        assert!(cancel.is_cancelled(), "key removal must cancel active sync");
        assert!(
            !removal.is_finished(),
            "key removal must wait for sync exit"
        );

        drop(sync_guard);
        removal.await.unwrap().unwrap();
        ACTIVE_RUNS.remove(KEY);
    }

    #[tokio::test]
    async fn disconnect_waits_for_an_active_sync_before_erasing_local_data() {
        let _test_guard = SYNC_TEST_LOCK.lock().await;
        let sync_guard = try_sync_guard().unwrap();
        let db = Arc::new(
            DatabaseManager::new("sqlite::memory:", Default::default())
                .await
                .unwrap(),
        );
        db.connector_upsert_object(&ConnectorObjectDraft {
            connector: CONNECTOR_ID.to_string(),
            namespace: KEY.to_string(),
            object_kind: "book".to_string(),
            object_id: "book-1".to_string(),
            revision: None,
            title: Some("A book".to_string()),
            body_text: "imported before disconnect".to_string(),
            content_hash: None,
            event_at: None,
            fetched_at: Utc::now(),
            source_url: None,
        })
        .await
        .unwrap();
        let service = WeReadService::new(db.clone(), None);
        let cancel = CancellationToken::new();
        ACTIVE_RUNS.insert(KEY.to_string(), cancel.clone());

        let disconnect = tokio::spawn(async move { service.disconnect("erase").await });
        tokio::task::yield_now().await;
        assert!(cancel.is_cancelled(), "disconnect must cancel active sync");
        assert!(
            !disconnect.is_finished(),
            "disconnect must wait for sync exit before erasing"
        );

        drop(sync_guard);
        disconnect.await.unwrap().unwrap();
        ACTIVE_RUNS.remove(KEY);
        assert_eq!(
            db.connector_imported_object_count(CONNECTOR_ID)
                .await
                .unwrap(),
            0
        );
    }
}
