// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Office connector: engine-side orchestration for the read-only Feishu and
//! Tencent Meeting connections. Owns connection/scope state, drives CLI reads
//! through `screenpipe_connect::office::runner`, and persists imported
//! objects into the self-contained `office_*` tables (body + FTS). Sync runs
//! as an independent in-process task — no knowledge worker/job dependency.

use std::sync::Arc;

use chrono::DateTime;
use serde::Serialize;
use screenpipe_connect::office::runner::CliRunner;
use screenpipe_connect::office::types::{
    OfficeAuthStatus, OfficeCommand, OfficeError, OfficeErrorCode,
    OfficeObjectKind, OfficeProvider, OfficeRuntimeStatus, OfficeScope, OfficeSyncStatus,
};
use screenpipe_connect::office::types::OfficeObject;
use screenpipe_db::{OfficeConnectionUpdate, OfficeObjectDraft, DatabaseManager};
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub mod routes;

/// Error type for office operations: `code` carries the stable REST/UI code
/// (reuses `OfficeErrorCode` so front-ends keep one mapping).
#[derive(Clone, Debug)]
pub struct OfficeServiceError {
    pub code: OfficeErrorCode,
    pub message: String,
    pub transient: bool,
    pub http: u16,
}

impl OfficeServiceError {
    pub fn new(code: OfficeErrorCode, message: impl Into<String>, http: u16) -> Self {
        Self {
            code,
            message: message.into(),
            transient: false,
            http,
        }
    }

    pub fn transient(code: OfficeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            transient: true,
            http: 500,
        }
    }

    pub fn as_str(&self) -> &'static str {
        self.code.as_str()
    }
}

impl From<sqlx::Error> for OfficeServiceError {
    fn from(e: sqlx::Error) -> Self {
        OfficeServiceError::new(
            OfficeErrorCode::ProviderError,
            format!("db_error:{e}"),
            500,
        )
    }
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[derive(Clone)]
pub struct OfficeService {
    pub db: Arc<DatabaseManager>,
    pub runner: CliRunner,
    pub cancel: CancellationToken,
}

impl OfficeService {
    pub fn new(db: Arc<DatabaseManager>, managed_dir: std::path::PathBuf) -> Self {
        Self {
            db,
            runner: CliRunner::new(managed_dir),
            cancel: CancellationToken::new(),
        }
    }

    fn provider(provider: &str) -> Result<OfficeProvider, OfficeServiceError> {
        OfficeProvider::from_str(provider).ok_or_else(|| {
            OfficeServiceError::new(
                OfficeErrorCode::ScopeInvalid,
                format!("未知办公平台 {provider}"),
                400,
            )
        })
    }

    /// Aggregate status for the connection card. Light: DB state + cached
    /// dependency info; the live account probe only runs on explicit refresh.
    pub async fn status(&self, provider: &str) -> Result<OfficeStatusDto, OfficeServiceError> {
        let provider = Self::provider(provider)?;
        self.db.office_ensure_connection(provider.as_str()).await?;
        let row = self
            .db
            .office_get_connection(provider.as_str())
            .await?
            .ok_or_else(|| {
                OfficeServiceError::new(
                    OfficeErrorCode::ProviderError,
                    "连接行初始化失败",
                    500,
                )
            })?;
        let scope = self
            .db
            .office_get_scope(provider.as_str())
            .await?
            .and_then(|(json, _)| serde_json::from_str::<OfficeScope>(&json).ok())
            .unwrap_or_default();
        let imported = self.db.office_imported_object_count(provider.as_str()).await?;
        Ok(Self::dto(provider, &row, scope, imported))
    }

    pub async fn status_all(&self) -> Result<Vec<OfficeStatusDto>, OfficeServiceError> {
        let mut out = Vec::new();
        for p in [OfficeProvider::Feishu, OfficeProvider::TencentMeeting] {
            out.push(self.status(p.as_str()).await?);
        }
        Ok(out)
    }

    fn dto(
        provider: OfficeProvider,
        row: &screenpipe_db::OfficeConnectionRow,
        scope: OfficeScope,
        imported: i64,
    ) -> OfficeStatusDto {
        OfficeStatusDto {
            provider: provider.as_str().to_string(),
            runtime_status: row.runtime_status.clone(),
            auth_status: row.auth_status.clone(),
            sync_status: row.sync_status.clone(),
            cli_version: row.cli_version.clone(),
            account_alias: row.account_namespace.clone(),
            scope,
            scope_revision: row.scope_revision,
            last_sync_at: row.last_sync_at.clone(),
            last_success_at: row.last_success_at.clone(),
            imported_objects: imported.max(0) as u64,
            last_error_code: row.last_error_code.clone(),
            last_error_message: row.last_error_message.clone(),
        }
    }

    /// Re-probe the CLI dependency and the live account. Explicit action.
    pub async fn refresh(&self, provider: &str) -> Result<OfficeStatusDto, OfficeServiceError> {
        let provider = Self::provider(provider)?;
        self.db.office_ensure_connection(provider.as_str()).await?;
        let dependency = self.runner.discover(provider).await;
        let mut updates = OfficeConnectionUpdate {
            cli_path: dependency.path.clone(),
            cli_version: dependency.version.clone(),
            runtime_status: Some(
                serde_json::to_value(&dependency.runtime_status)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "missing".to_string()),
            ),
            ..Default::default()
        };
        if dependency.runtime_status == OfficeRuntimeStatus::Supported {
            let bound = self
                .db
                .office_get_connection(provider.as_str())
                .await
                .ok()
                .flatten()
                .and_then(|r| r.account_namespace);
            match self
                .runner
                .account_matches(&dependency, self.cancel.child_token(), bound.as_deref())
                .await
            {
                Ok(identity) => {
                    updates.account_namespace = Some(identity.account_id.clone());
                    updates.auth_status =
                        Some(OfficeAuthStatus::Authorized.as_str().to_string());
                    updates.last_error_code = None;
                    updates.last_error_message = None;
                    updates.clear_errors = true;
                }
                Err(e) => {
                    updates.auth_status = Some(
                        match e.code {
                            OfficeErrorCode::AccountMismatch => OfficeAuthStatus::Expired,
                            _ => OfficeAuthStatus::Disconnected,
                        }
                        .as_str()
                        .to_string(),
                    );
                    updates.last_error_code = Some(e.code.as_str().to_string());
                    updates.last_error_message = Some(e.message.clone());
                }
            }
        }
        self.db
            .office_update_connection(provider.as_str(), updates)
            .await?;
        self.status(provider.as_str()).await
    }

    /// Save the user-approved scope. Validation is strict: only provider
    /// object ids/links are accepted; an empty scope is allowed (nothing to
    /// read). Saving never triggers a backfill by itself.
    pub async fn save_scope(
        &self,
        provider: &str,
        expected_revision: i64,
        scope: OfficeScope,
    ) -> Result<i64, OfficeServiceError> {
        let provider_enum = Self::provider(provider)?;
        if provider_enum == OfficeProvider::Feishu && scope.meeting_ids.len() > 0 {
            return Err(OfficeServiceError::new(
                OfficeErrorCode::ScopeInvalid,
                "会议范围只属于腾讯会议连接",
                400,
            ));
        }
        if provider_enum == OfficeProvider::TencentMeeting
            && (!scope.document_ids.is_empty()
                || !scope.chat_ids.is_empty()
                || scope.all_accessible_chats
                || scope.sync_calendar_events)
        {
            return Err(OfficeServiceError::new(
                OfficeErrorCode::ScopeInvalid,
                "文档/会话/日历范围只属于飞书连接",
                400,
            ));
        }
        if scope.window_end_ms > 0 && scope.window_start_ms > scope.window_end_ms {
            return Err(OfficeServiceError::new(
                OfficeErrorCode::ScopeInvalid,
                "时间窗口不合法",
                400,
            ));
        }
        for doc in &scope.document_ids {
            if !is_feishu_doc_ref(doc) {
                return Err(OfficeServiceError::new(
                    OfficeErrorCode::ScopeInvalid,
                    format!("不是有效的飞书文档: {doc}"),
                    400,
                ));
            }
        }
        for chat in &scope.chat_ids {
            if !chat.starts_with("oc_") {
                return Err(OfficeServiceError::new(
                    OfficeErrorCode::ScopeInvalid,
                    format!("不是有效的会话 ID: {chat}"),
                    400,
                ));
            }
        }
        for meeting in &scope.meeting_ids {
            if meeting.is_empty() {
                return Err(OfficeServiceError::new(
                    OfficeErrorCode::ScopeInvalid,
                    "会议 ID 不能为空",
                    400,
                ));
            }
        }
        let row = self.db.office_get_connection(provider).await?;
        if let Some(row) = row {
            if row.scope_revision != expected_revision {
                return Err(OfficeServiceError::new(
                    OfficeErrorCode::ScopeInvalid,
                    "范围已被其他人修改，请刷新后重试",
                    409,
                ));
            }
        }
        let scope_json = serde_json::to_string(&scope).map_err(|e| {
            OfficeServiceError::new(OfficeErrorCode::ScopeInvalid, e.to_string(), 400)
        })?;
        let revision = self.db.office_save_scope(provider, &scope_json).await?;

        // Shrink immediately disables out-of-scope objects.
        let account = self
            .db
            .office_get_connection(provider)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.account_namespace)
            .unwrap_or_default();
        let keep: Vec<(String, String)> = scope
            .chat_ids
            .iter()
            .map(|c| ("message".to_string(), c.clone()))
            .collect();
        let disabled = self
            .db
            .office_disable_out_of_scope(provider, &account, &keep)
            .await?;
        let _ = disabled;
        Ok(revision)
    }

    /// Start a manual sync for the current scope. The sync runs as an
    /// independent in-process task; this call returns immediately with a
    /// run id. Runs are serialized per provider: a sync already in flight
    /// rejects a new one with `busy`.
    pub async fn start_sync(
        &self,
        provider: &str,
        expected_revision: i64,
    ) -> Result<i64, OfficeServiceError> {
        let provider_enum = Self::provider(provider)?;
        let row = self
            .db
            .office_get_connection(provider)
            .await?
            .ok_or_else(|| {
                OfficeServiceError::new(
                    OfficeErrorCode::ScopeInvalid,
                    "连接不存在",
                    400,
                )
            })?;
        if row.scope_revision != expected_revision {
            return Err(OfficeServiceError::new(
                OfficeErrorCode::ScopeInvalid,
                "范围已变化，请刷新后重试",
                409,
            ));
        }
        if row.auth_status != OfficeAuthStatus::Authorized.as_str() {
            return Err(OfficeServiceError::new(
                OfficeErrorCode::AuthExpired,
                "尚未授权，无法同步",
                400,
            ));
        }
        if row.sync_status == OfficeSyncStatus::Running.as_str()
            || row.sync_status == OfficeSyncStatus::Queued.as_str()
        {
            return Err(OfficeServiceError::new(
                OfficeErrorCode::RateLimited,
                "同步正在进行中",
                429,
            ));
        }
        let scope = self
            .db
            .office_get_scope(provider)
            .await?
            .map(|(json, _)| json)
            .unwrap_or_default();
        if serde_json::from_str::<OfficeScope>(&scope)
            .map(|s| s.is_empty())
            .unwrap_or(true)
        {
            return Err(OfficeServiceError::new(
                OfficeErrorCode::ScopeInvalid,
                "请先选择要读取的范围",
                400,
            ));
        }
        let run_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        self.db
            .office_update_connection(
                provider,
                OfficeConnectionUpdate {
                    sync_status: Some(OfficeSyncStatus::Running.as_str().to_string()),
                    ..Default::default()
                },
            )
            .await?;
        let service = self.clone();
        let provider_owned = provider.to_string();
        let cancel_token = self.cancel.child_token();
        tokio::spawn(async move {
            let summary = run_sync(&service, &provider_owned, &scope, cancel_token.clone()).await;
            let (sync_status, error_code, error_message) = match &summary {
                Ok(s) => (
                    if s.partial {
                        OfficeSyncStatus::Partial
                    } else {
                        OfficeSyncStatus::Idle
                    },
                    None,
                    None,
                ),
                Err(e) => (
                    OfficeSyncStatus::Failed,
                    Some(e.code.as_str().to_string()),
                    Some(e.message.clone()),
                ),
            };
            let _ = service
                .db
                .office_update_connection(
                    &provider_owned,
                    OfficeConnectionUpdate {
                        sync_status: Some(sync_status.as_str().to_string()),
                        last_sync_at: Some(now_iso()),
                        last_success_at: if summary.is_ok() {
                            Some(now_iso())
                        } else {
                            None
                        },
                        last_error_code: error_code,
                        last_error_message: error_message,
                        clear_errors: summary.is_ok(),
                        ..Default::default()
                    },
                )
                .await;
            let _ = provider_enum;
        });
        Ok(run_id)
    }

    /// Disconnect: stop reading, disable imported objects; `erase` also
    /// deletes imported content with re-import suppression. Never touches the
    /// user's global CLI login.
    pub async fn disconnect(&self, provider: &str, local_data: &str) -> Result<(), OfficeServiceError> {
        Self::provider(provider)?;
        if local_data == "erase" {
            self.db.office_erase(provider).await?;
        } else {
            // Default: retain inactive copies (objects stay, disabled).
            self.db
                .office_disable_out_of_scope(provider, "", &[])
                .await?;
        }
        self.db
            .office_update_connection(
                provider,
                OfficeConnectionUpdate {
                    auth_status: Some(OfficeAuthStatus::Disconnected.as_str().to_string()),
                    sync_status: Some(OfficeSyncStatus::Idle.as_str().to_string()),
                    enabled: Some(false),
                    auto_sync: Some(false),
                    bump_connection_revision: true,
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }

    /// Search imported office objects (used by the desktop search surface).
    pub async fn search(
        &self,
        provider: Option<&str>,
        query: &str,
        limit: u32,
    ) -> Result<Vec<screenpipe_db::OfficeObjectRow>, OfficeServiceError> {
        let hits = self.db.office_search(provider, query, limit).await?;
        Ok(hits.into_iter().map(|(row, _)| row).collect())
    }
}

fn is_feishu_doc_ref(s: &str) -> bool {
    s.starts_with("doccn")
        || s.starts_with("doxcn")
        || s.starts_with("docs/")
        || s.contains("feishu.cn/docx/")
        || s.contains("feishu.cn/doc/")
        || s.contains("feishu.cn/wiki/")
        || s.starts_with("wiki/")
}

/// REST shape for the connection card (mirrored by office-types.ts).
#[derive(Clone, Debug, Serialize)]
pub struct OfficeStatusDto {
    pub provider: String,
    pub runtime_status: String,
    pub auth_status: String,
    pub sync_status: String,
    pub cli_version: Option<String>,
    pub account_alias: Option<String>,
    pub scope: OfficeScope,
    pub scope_revision: i64,
    pub last_sync_at: Option<String>,
    pub last_success_at: Option<String>,
    pub imported_objects: u64,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
}

pub(crate) struct SyncSummary {
    pub imported: u64,
    pub partial: bool,
}

/// One sync execution: scope-driven windowed reads with per-page commits.
/// Window high-water advances only on fully completed page sets; cursors and
/// objects commit through the same path.
async fn run_sync(
    service: &OfficeService,
    provider: &str,
    scope_json: &str,
    cancel: CancellationToken,
) -> Result<SyncSummary, OfficeError> {
    let provider_enum = OfficeProvider::from_str(provider)
        .ok_or_else(|| OfficeError::new(OfficeErrorCode::ScopeInvalid, "未知平台"))?;
    let scope: OfficeScope = serde_json::from_str(scope_json)
        .map_err(|_| OfficeError::new(OfficeErrorCode::ScopeInvalid, "范围数据损坏"))?;
    let dependency = service.runner.discover(provider_enum).await;
    // Verify the CLI is still logged into the same account before reading.
    let bound = service
        .db
        .office_get_connection(provider)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.account_namespace);
    let identity = service
        .runner
        .account_matches(&dependency, cancel.clone(), bound.as_deref())
        .await?;
    service
        .db
        .office_update_connection(
            provider,
            OfficeConnectionUpdate {
                sync_status: Some(OfficeSyncStatus::Running.as_str().to_string()),
                account_namespace: Some(identity.account_id.clone()),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| OfficeError::new(OfficeErrorCode::ProviderError, e.to_string()))?;

    let mut imported: u64 = 0;
    let mut partial = false;
    match provider_enum {
        OfficeProvider::Feishu => {
            // Messages: per chat, windowed pages; cursor per chat.
            // "All accessible chats" expands the whitelist to every chat the
            // account can see (paged), reusing the UI picker's list command.
            let mut chats = scope.chat_ids.clone();
            if scope.all_accessible_chats {
                let mut cursor: Option<String> = None;
                loop {
                    if cancel.is_cancelled() {
                        return Err(OfficeError::new(OfficeErrorCode::CliTimeout, "同步已取消"));
                    }
                    let command = OfficeCommand::FeishuChatList { cursor, limit: 100 };
                    let output = service
                        .runner
                        .run(&dependency, &command, cancel.clone())
                        .await?;
                    if output.truncated {
                        partial = true;
                        break;
                    }
                    let payload = cli_payload(&output);
                    let (found, next) =
                        screenpipe_connect::office::feishu::parse_chat_list(&payload)?;
                    chats.extend(found.into_iter().map(|(id, _)| id));
                    match next {
                        Some(next) => cursor = Some(next),
                        None => break,
                    }
                }
                chats.sort();
                chats.dedup();
            }
            let window_start = if scope.window_start_ms > 0 {
                ms_to_iso(scope.window_start_ms)
            } else {
                default_window_start()
            };
            let window_end = if scope.window_end_ms > 0 {
                ms_to_iso(scope.window_end_ms)
            } else {
                now_iso()
            };
            for chat_id in &chats {
                let cursor_key = format!("chat:{chat_id}:messages");
                let start = window_start.clone();
                let end = window_end.clone();
                // lark page tokens are opaque and scoped to one sync run —
                // never replay a stored high-water as a page token. Each run
                // walks its full window; object upserts absorb the overlap.
                // The stored high-water stays as diagnostics only.
                let _ = service.db.office_get_cursor(provider, &cursor_key).await;
                let mut cursor: Option<String> = None;
                loop {
                    if cancel.is_cancelled() {
                        return Err(OfficeError::new(OfficeErrorCode::CliTimeout, "同步已取消"));
                    }
                    let command = OfficeCommand::FeishuMessages {
                        chat_id: chat_id.clone(),
                        start_iso: start.clone(),
                        end_iso: end.clone(),
                        cursor: cursor.clone(),
                        limit: 50,
                    };
                    let output = service
                        .runner
                        .run(&dependency, &command, cancel.clone())
                        .await?;
                    if output.truncated {
                        partial = true;
                        break;
                    }
                    let payload = cli_payload(&output);
                    let page = screenpipe_connect::office::feishu::parse_chat_messages(
                        &payload,
                        &identity.account_id,
                        chat_id,
                    )?;
                    for object in &page.objects {
                        if register_object(service, object).await? {
                            imported += 1;
                        }
                    }
                    if page.complete {
                        service
                            .db
                            .office_set_cursor(provider, &cursor_key, &end)
                            .await
                            .map_err(|e| {
                                OfficeError::new(OfficeErrorCode::ProviderError, e.to_string())
                            })?;
                        break;
                    }
                    match &page.next_cursor {
                        Some(next) => cursor = Some(next.clone()),
                        None => break,
                    }
                }
            }
            // Calendar: primary calendar events within the window. The
            // calendar domain needs its own CLI login, so a failure here is
            // best-effort (partial) rather than failing the whole run.
            if scope.sync_calendar_events {
                let command = OfficeCommand::FeishuCalendarEvents {
                    start_iso: window_start.clone(),
                    end_iso: window_end.clone(),
                };
                match service
                    .runner
                    .run(&dependency, &command, cancel.clone())
                    .await
                {
                    Ok(output) => {
                        if output.truncated {
                            partial = true;
                        } else {
                            let payload = cli_payload(&output);
                            match screenpipe_connect::office::feishu::parse_calendar_events(
                                &payload,
                                &identity.account_id,
                            ) {
                                Ok(page) => {
                                    for object in &page.objects {
                                        if register_object(service, object).await? {
                                            imported += 1;
                                        }
                                    }
                                }
                                Err(e) => {
                                    partial = true;
                                    warn!("feishu calendar parse failed: {}", e.message);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        partial = true;
                        warn!(
                            "feishu calendar fetch failed (需要 `lark-cli auth login --domain calendar`?): {}",
                            e.message
                        );
                    }
                }
            }
            // Documents: current-version fetch per explicit doc.
            for doc_ref in &scope.document_ids {
                if cancel.is_cancelled() {
                    break;
                }
                let command = OfficeCommand::FeishuDocFetch {
                    doc_id: doc_ref.clone(),
                };
                let output = service
                    .runner
                    .run(&dependency, &command, cancel.clone())
                    .await?;
                if output.truncated {
                    partial = true;
                    continue;
                }
                let payload = cli_payload(&output);
                match screenpipe_connect::office::feishu::parse_doc_fetch(
                    &payload,
                    &identity.account_id,
                    doc_ref,
                ) {
                    Ok(object) => {
                        if register_object(service, &object).await? {
                            imported += 1;
                        }
                    }
                    Err(e) if e.code == OfficeErrorCode::CapabilityMissing => {
                        partial = true;
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        OfficeProvider::TencentMeeting => {
            let meetings: Vec<(String, String)> = if scope.all_accessible_meetings {
                // List meetings in the window with its own page token.
                let mut list = Vec::new();
                let mut next_token: Option<String> = None;
                loop {
                    let command = OfficeCommand::TencentMeetingList {
                        start_iso: ms_to_iso(scope.window_start_ms),
                        end_iso: if scope.window_end_ms > 0 {
                            ms_to_iso(scope.window_end_ms)
                        } else {
                            now_iso()
                        },
                        cursor: next_token.clone(),
                        limit: 20,
                    };
                    let output = service
                        .runner
                        .run(&dependency, &command, cancel.clone())
                        .await?;
                    let (mut items, next, complete) =
                        screenpipe_connect::office::tencent_meeting::parse_meeting_list(
                            &cli_payload(&output),
                        )?;
                    list.append(&mut items);
                    if complete || next.is_none() {
                        break;
                    }
                    next_token = next;
                }
                list.into_iter().map(|(id, subject, _)| (id, subject)).collect()
            } else {
                scope
                    .meeting_ids
                    .iter()
                    .map(|id| (id.clone(), String::new()))
                    .collect()
            };
            for (meeting_id, _subject) in meetings {
                if cancel.is_cancelled() {
                    break;
                }
                let command = OfficeCommand::TencentRecordList {
                    meeting_id: meeting_id.clone(),
                    cursor: None,
                    limit: 30,
                };
                let output = service
                    .runner
                    .run(&dependency, &command, cancel.clone())
                    .await;
                let recordings = match output {
                    Ok(out) => {
                        let (mut recordings, mut next, mut complete) =
                            screenpipe_connect::office::tencent_meeting::parse_record_list(
                                &out.stdout,
                            )?;
                        while !complete {
                            let command = OfficeCommand::TencentRecordList {
                                meeting_id: meeting_id.clone(),
                                cursor: next.clone(),
                                limit: 30,
                            };
                            let out = service
                                .runner
                                .run(&dependency, &command, cancel.clone())
                                .await?;
                            let (mut more, token, done) =
                                screenpipe_connect::office::tencent_meeting::parse_record_list(
                                    &out.stdout,
                                )?;
                            recordings.append(&mut more);
                            next = token;
                            complete = done;
                            if next.is_none() {
                                break;
                            }
                        }
                        recordings
                    }
                    Err(e) if e.code == OfficeErrorCode::RecordingMissing => {
                        partial = true;
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                for recording_id in recordings {
                    let command = OfficeCommand::TencentTranscriptParagraphs {
                        recording_id: recording_id.clone(),
                    };
                    let output = service
                        .runner
                        .run(&dependency, &command, cancel.clone())
                        .await;
                    let (transcript, transcript_ok) = match output {
                        Ok(out) => {
                            let page = screenpipe_connect::office::tencent_meeting::parse_transcript_paragraphs(
                                &cli_payload(&out),
                            )?;
                            (page.text, true)
                        }
                        Err(e) if e.code == OfficeErrorCode::TranscriptPending => {
                            partial = true;
                            (String::new(), false)
                        }
                        Err(_e) => {
                            partial = true;
                            (String::new(), false)
                        }
                    };
                    if transcript_ok && !transcript.trim().is_empty() {
                        let object = OfficeObject {
                            provider: OfficeProvider::TencentMeeting,
                            account_namespace: identity.account_id.clone(),
                            object_kind: OfficeObjectKind::Transcript,
                            object_id: recording_id.clone(),
                            revision: None,
                            title: Some(format!("会议转写 {meeting_id}")),
                            body_text: transcript,
                            event_at: (scope.window_start_ms > 0)
                                .then(|| ms_to_utc(scope.window_start_ms)),
                            source_url: None,
                            actor: None,
                            anchors: vec![
                                ("meeting_id".to_string(), meeting_id.clone()),
                                ("recording_id".to_string(), recording_id.clone()),
                            ],
                            completeness: if partial {
                                screenpipe_connect::office::types::OfficeCompleteness::Partial {
                                    reason: OfficeErrorCode::PaginationIncomplete,
                                }
                            } else {
                                screenpipe_connect::office::types::OfficeCompleteness::Full
                            },
                            platform_generated: false,
                        };
                        if register_object(service, &object).await? {
                            imported += 1;
                        }
                    }
                    // Smart minutes: separate derived source, marked as such.
                    let command = OfficeCommand::TencentSmartMinutes {
                        recording_id: recording_id.clone(),
                    };
                    if let Ok(out) = service
                        .runner
                        .run(&dependency, &command, cancel.clone())
                        .await
                    {
                        if let Ok(minutes) =
                            screenpipe_connect::office::tencent_meeting::parse_smart_minutes(
                                &cli_payload(&out),
                                &identity.account_id,
                                &recording_id,
                            )
                        {
                            if register_object(service, &minutes).await? {
                                imported += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(SyncSummary { imported, partial })
}

/// Register one normalized object into the office tables (body + FTS).
/// Returns true when the object was new (first import).
async fn register_object(
    service: &OfficeService,
    object: &OfficeObject,
) -> Result<bool, OfficeError> {
    let kind = match object.object_kind {
        OfficeObjectKind::Message => "message",
        OfficeObjectKind::Document => "document",
        OfficeObjectKind::Transcript => "transcript",
        OfficeObjectKind::Summary => "summary",
        _ => "document",
    };
    let completeness = match &object.completeness {
        screenpipe_connect::office::types::OfficeCompleteness::Full => "full".to_string(),
        screenpipe_connect::office::types::OfficeCompleteness::Partial { reason } => {
            format!("partial:{}", reason.as_str())
        }
    };
    let draft = OfficeObjectDraft {
        provider: object.provider.as_str().to_string(),
        account_namespace: object.account_namespace.clone(),
        object_kind: kind.to_string(),
        object_id: object.object_id.clone(),
        revision: object.revision.clone(),
        title: object.title.clone(),
        body_text: object.body_text.clone(),
        completeness,
        event_at: object.event_at,
        fetched_at: chrono::Utc::now(),
        source_url: object.source_url.clone(),
        activity_anchor: object
            .anchors
            .first()
            .map(|(k, v)| format!("{k}:{v}")),
        platform_generated: object.platform_generated,
    };
    service
        .db
        .office_upsert_object(&draft)
        .await
        .map_err(|e| OfficeError::new(OfficeErrorCode::ProviderError, e.to_string()))
}

/// Prefer stdout; when the CLI wrote an error envelope to stderr instead,
/// parse from there so typed error codes survive.
fn cli_payload(output: &screenpipe_connect::office::types::OfficeCliOutput) -> String {
    if output.stdout.trim().is_empty() && !output.stderr.trim().is_empty() {
        output.stderr.clone()
    } else {
        output.stdout.clone()
    }
}

fn ms_to_iso(ms: i64) -> String {
    ms_to_utc(ms).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn ms_to_utc(ms: i64) -> DateTime<chrono::Utc> {
    DateTime::from_timestamp_millis(ms).unwrap_or_else(chrono::Utc::now)
}

fn default_window_start() -> String {
    (chrono::Utc::now() - chrono::Duration::hours(24 * 7))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}