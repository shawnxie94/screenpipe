// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Optional Graphiti transport. This module only runs when explicitly opted in;
//! capture and SQLite write paths never wait for these network requests.

use chrono::{DateTime, Utc};
use reqwest::{redirect::Policy, Client, Url};
use screenpipe_db::{ActivityEvidenceRecord, ActivityIntervalRecord, ActivitySummaryEvidenceRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    future::Future,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::Duration,
};
use tokio::time::sleep;

const AUTO_SYNC_PAGE_SIZE: i64 = 1;
const AUTO_SYNC_PER_CYCLE: usize = 3;
const AUTO_SYNC_INTERVAL: Duration = Duration::from_secs(30);
const AUTO_SYNC_MAX_BACKOFF: Duration = Duration::from_secs(15 * 60);
const AUTO_SYNC_HTTP_TIMEOUT: Duration = Duration::from_secs(310);
const SEARCH_HTTP_TIMEOUT: Duration = Duration::from_secs(2);
const STATE_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const CHECKPOINT_FILE: &str = "graphiti-auto-sync-state.json";
const SETTINGS_FILE: &str = "graphiti-settings.json";
const SETTINGS_SYNC_INTERVAL_DEFAULT_SECONDS: u64 = 30;
const SETTINGS_SYNC_INTERVAL_MIN_SECONDS: u64 = 30;
const SETTINGS_SYNC_INTERVAL_MAX_SECONDS: u64 = 24 * 60 * 60;

#[derive(Clone, Debug)]
struct GraphitiSettings {
    auto_sync_enabled: bool,
    search_enabled: bool,
    endpoint: Option<Url>,
    sync_interval: Duration,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedGraphitiSettings {
    adapter_url: Option<String>,
    allow_http_localhost: bool,
    auto_sync_enabled: bool,
    search_enabled: bool,
    sync_interval_seconds: u64,
}

impl GraphitiSettings {
    fn from_env() -> Self {
        let auto_sync_enabled = env_flag("SCREENPIPE_GRAPHITI_AUTO_SYNC_ENABLED");
        let search_enabled = env_flag("SCREENPIPE_GRAPHITI_SEARCH_ENABLED");
        let allow_http_localhost = env_flag("SCREENPIPE_GRAPHITI_ALLOW_HTTP_LOCALHOST");
        let endpoint = std::env::var("SCREENPIPE_GRAPHITI_ADAPTER_URL")
            .ok()
            .and_then(|raw| validate_adapter_url(&raw, allow_http_localhost));
        if (auto_sync_enabled || search_enabled) && endpoint.is_none() {
            tracing::warn!(
                "Graphiti opt-in is enabled but adapter configuration is missing or unsupported; no requests will be sent"
            );
        }
        Self {
            auto_sync_enabled,
            search_enabled,
            endpoint,
            sync_interval: Duration::from_secs(SETTINGS_SYNC_INTERVAL_DEFAULT_SECONDS),
        }
    }

    fn load(data_dir: &Path) -> Self {
        let contents = match fs::read(data_dir.join(SETTINGS_FILE)) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Self::from_env(),
            Err(_) => return Self::disabled(),
        };
        let Ok(settings) = serde_json::from_slice::<PersistedGraphitiSettings>(&contents) else {
            tracing::warn!("Graphiti settings file is invalid; Graphiti remains disabled");
            return Self::disabled();
        };
        if !(SETTINGS_SYNC_INTERVAL_MIN_SECONDS..=SETTINGS_SYNC_INTERVAL_MAX_SECONDS)
            .contains(&settings.sync_interval_seconds)
        {
            tracing::warn!("Graphiti sync interval is invalid; Graphiti remains disabled");
            return Self::disabled();
        }
        let endpoint = settings
            .adapter_url
            .as_deref()
            .and_then(|raw| validate_adapter_url(raw, settings.allow_http_localhost));
        if (settings.auto_sync_enabled || settings.search_enabled) && endpoint.is_none() {
            tracing::warn!(
                "Graphiti is enabled but its adapter URL is invalid; Graphiti remains disabled"
            );
            return Self::disabled();
        }
        Self {
            auto_sync_enabled: settings.auto_sync_enabled,
            search_enabled: settings.search_enabled,
            endpoint,
            sync_interval: Duration::from_secs(settings.sync_interval_seconds),
        }
    }

    fn disabled() -> Self {
        Self {
            auto_sync_enabled: false,
            search_enabled: false,
            endpoint: None,
            sync_interval: Duration::from_secs(SETTINGS_SYNC_INTERVAL_DEFAULT_SECONDS),
        }
    }
}

struct GraphitiClient {
    client: Option<Client>,
}

impl GraphitiClient {
    fn from_env() -> Self {
        let client = Client::builder()
            .redirect(Policy::none())
            .timeout(AUTO_SYNC_HTTP_TIMEOUT)
            .build()
            .ok();
        Self { client }
    }

    async fn search(
        &self,
        settings: &GraphitiSettings,
        query: &str,
        limit: u32,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<Vec<GraphitiSearchHit>, ()> {
        if !settings.search_enabled {
            return Err(());
        }
        let endpoint = settings
            .endpoint
            .as_ref()
            .ok_or(())?
            .join("v1/search")
            .map_err(|_| ())?;
        let client = self.client.as_ref().ok_or(())?;
        let response = client
            .post(endpoint)
            .timeout(SEARCH_HTTP_TIMEOUT)
            .json(&GraphitiSearchRequest {
                query,
                limit: limit.clamp(1, 20),
                start_time,
                end_time,
            })
            .send()
            .await
            .map_err(|_| ())?
            .error_for_status()
            .map_err(|_| ())?
            .json::<GraphitiSearchResponse>()
            .await
            .map_err(|_| ())?;
        Ok(response.hits)
    }
}

static GRAPHITI_CLIENT: OnceLock<GraphitiClient> = OnceLock::new();
static AUTO_SYNC_WORKERS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

fn graphiti_client() -> &'static GraphitiClient {
    GRAPHITI_CLIENT.get_or_init(GraphitiClient::from_env)
}

pub(crate) fn search_enabled(data_dir: &Path) -> bool {
    GraphitiSettings::load(data_dir).search_enabled
}

pub(crate) async fn search(
    data_dir: &Path,
    query: &str,
    limit: u32,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
) -> Result<Vec<GraphitiSearchHit>, ()> {
    let settings = GraphitiSettings::load(data_dir);
    graphiti_client()
        .search(&settings, query, limit, start_time, end_time)
        .await
}

/// Start one bounded exporter for this AppState. The worker holds only a weak
/// reference to AppState so it exits when the local API/router is torn down.
pub(crate) fn ensure_auto_sync(state: &Arc<crate::server::AppState>) {
    let service = graphiti_client();
    let Some(client) = service.client.clone() else {
        return;
    };
    let data_dir = state.screenpipe_dir.clone();
    let key = data_dir.clone();
    let workers = AUTO_SYNC_WORKERS.get_or_init(|| Mutex::new(HashSet::new()));
    let mut workers = workers
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !workers.insert(key.clone()) {
        return;
    }
    let state = Arc::downgrade(state);
    tokio::spawn(async move {
        auto_sync_loop(state, data_dir, client).await;
        if let Some(workers) = AUTO_SYNC_WORKERS.get() {
            workers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&key);
        }
    });
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn validate_adapter_url(raw: &str, allow_http_localhost: bool) -> Option<Url> {
    let url = Url::parse(raw.trim()).ok()?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let secure_tailnet =
        url.scheme() == "https" && host.len() > ".ts.net".len() && host.ends_with(".ts.net");
    let allowed_loopback = allow_http_localhost
        && url.scheme() == "http"
        && matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]" | "::1");
    (secure_tailnet || allowed_loopback).then_some(url)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct AutoSyncCheckpoint {
    version: u8,
    cursor: Option<GraphitiCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct GraphitiCursor {
    updated_at: String,
    interval_id: i64,
}

struct SummaryExport {
    interval: ActivityIntervalRecord,
    evidence_refs: Vec<ActivitySummaryEvidenceRef>,
    source_refs: Vec<ActivityEvidenceRecord>,
}

#[derive(Serialize)]
struct GraphitiSearchRequest<'a> {
    query: &'a str,
    limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_time: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_time: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct GraphitiSearchResponse {
    hits: Vec<GraphitiSearchHit>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphitiSearchHit {
    pub uuid: String,
    pub fact: String,
    pub reference_time: DateTime<Utc>,
    pub source_refs: Vec<GraphitiSourceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphitiSourceRef {
    pub source_type: String,
    pub source_id: i64,
    pub occurred_at: DateTime<Utc>,
    #[serde(default)]
    pub frame_id: Option<i64>,
    #[serde(default)]
    pub app_name: Option<String>,
    #[serde(default)]
    pub window_title: Option<String>,
    #[serde(default)]
    pub browser_url: Option<String>,
}

impl From<GraphitiSourceRef> for super::retrieval::fusion::SearchSourceRef {
    fn from(source: GraphitiSourceRef) -> Self {
        Self {
            source_type: source.source_type,
            source_id: source.source_id,
            occurred_at: source.occurred_at.to_rfc3339(),
            frame_id: source.frame_id,
            app_name: source.app_name,
            window_title: source.window_title,
            browser_url: source.browser_url.and_then(|url| redact_url(&url)),
        }
    }
}

#[derive(Debug, Serialize)]
struct GraphitiEpisode {
    episode_id: String,
    name: String,
    reference_time: String,
    source_description: &'static str,
    episode_body: GraphitiEpisodeBody,
    source_refs: Vec<GraphitiEpisodeSourceRef>,
    privacy: &'static str,
    payload_hash: String,
}

#[derive(Debug, Serialize)]
struct GraphitiEpisodeBody {
    interval_id: i64,
    kind: String,
    activity_type: String,
    start_at: String,
    end_at: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    keywords: Vec<String>,
    project_refs: Vec<String>,
    outcomes: Vec<serde_json::Value>,
    confidence: f64,
    status: String,
    producer: String,
}

#[derive(Debug, Serialize)]
struct GraphitiEpisodeSourceRef {
    source_type: String,
    source_id: i64,
    occurred_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    browser_url: Option<String>,
}

fn build_episode(export: &SummaryExport) -> Option<GraphitiEpisode> {
    let interval = &export.interval;
    let summary = interval
        .summary
        .clone()
        .filter(|summary| !summary.trim().is_empty())?;
    if interval.state != "final"
        || export.evidence_refs.is_empty()
        || export.evidence_refs.len() > 3
        || export.source_refs.len() != export.evidence_refs.len()
    {
        return None;
    }
    let cited: HashSet<(String, i64)> = export
        .evidence_refs
        .iter()
        .map(|reference| (reference.source_type.clone(), reference.source_id))
        .collect();
    let mut source_refs = Vec::with_capacity(export.source_refs.len());
    for source in &export.source_refs {
        if !cited.contains(&(source.source_type.clone(), source.source_id)) {
            return None;
        }
        source_refs.push(GraphitiEpisodeSourceRef {
            source_type: source.source_type.clone(),
            source_id: source.source_id,
            occurred_at: source.occurred_at.clone(),
            frame_id: source.frame_id,
            app_name: source.app_name.clone(),
            window_title: source.window_title.clone(),
            browser_url: source.browser_url.as_deref().and_then(redact_url),
        });
    }
    let activity_type = activity_type(interval, &summary);
    let body = GraphitiEpisodeBody {
        interval_id: interval.id,
        kind: interval.kind.clone(),
        activity_type: activity_type.clone(),
        start_at: interval.start_at.clone(),
        end_at: interval.end_at.clone(),
        title: interval.title.clone(),
        summary: Some(summary),
        keywords: interval.keywords.clone().unwrap_or_default(),
        project_refs: Vec::new(),
        outcomes: Vec::new(),
        confidence: interval.confidence.clamp(0.0, 1.0),
        status: "summarized".to_string(),
        producer: interval.producer.clone(),
    };
    let unsigned = serde_json::to_vec(&(&body, &source_refs, "private")).ok()?;
    let payload_hash = format!("sha256:{}", hex::encode(Sha256::digest(unsigned)));
    let name = if body.title.trim().is_empty() {
        activity_type
    } else {
        body.title.clone()
    };
    Some(GraphitiEpisode {
        episode_id: format!("screenpipe:activity:{}", interval.id),
        name,
        reference_time: interval.end_at.clone(),
        source_description: "screenpipe.activity_ledger",
        episode_body: body,
        source_refs,
        privacy: "private",
        payload_hash,
    })
}

fn activity_type(interval: &ActivityIntervalRecord, summary: &str) -> String {
    let text = format!("{} {summary}", interval.title).to_lowercase();
    if interval.kind.eq_ignore_ascii_case("meeting")
        || text.contains("会议")
        || text.contains("meeting")
    {
        "meeting"
    } else if ["research", "研究", "调研", "阅读", "read"]
        .iter()
        .any(|word| text.contains(word))
    {
        "research"
    } else if ["plan", "planning", "计划", "规划"]
        .iter()
        .any(|word| text.contains(word))
    {
        "planning"
    } else if ["code", "coding", "开发", "修复", "实现", "编程"]
        .iter()
        .any(|word| text.contains(word))
    {
        "implementation"
    } else if ["沟通", "邮件", "email", "chat", "消息"]
        .iter()
        .any(|word| text.contains(word))
    {
        "communication"
    } else {
        "unknown"
    }
    .to_string()
}

fn redact_url(raw: &str) -> Option<String> {
    let mut url = Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

#[derive(Deserialize)]
struct IngestResult {
    episode_id: String,
    status: String,
}

#[derive(Deserialize)]
struct IngestResponse {
    results: Vec<IngestResult>,
}

async fn auto_sync_loop(state: Weak<crate::server::AppState>, data_dir: PathBuf, client: Client) {
    let checkpoint_path = data_dir.join(CHECKPOINT_FILE);
    let mut checkpoint = match load_checkpoint(&checkpoint_path) {
        Ok(Some(checkpoint)) if checkpoint.version == 1 => Some(checkpoint),
        Ok(None) => None,
        Ok(Some(_)) | Err(_) => {
            tracing::warn!(
                "Graphiti auto-sync checkpoint is invalid; worker stopped without sending data"
            );
            return;
        }
    };

    let mut backoff = AUTO_SYNC_INTERVAL;
    loop {
        let Some(app_state) = state.upgrade() else {
            return;
        };
        let settings = GraphitiSettings::load(&app_state.screenpipe_dir);
        drop(app_state);
        if !settings.auto_sync_enabled || settings.endpoint.is_none() {
            if !wait_while_state_alive(&state, STATE_CHECK_INTERVAL).await {
                return;
            }
            continue;
        }
        let Some(base_url) = settings.endpoint.clone() else {
            continue;
        };
        if checkpoint.is_none() {
            let Some(app_state) = state.upgrade() else {
                return;
            };
            let cursor = app_state
                .db
                .activity_summary_export_latest_cursor()
                .await
                .map(|cursor| {
                    cursor.map(|(updated_at, interval_id)| GraphitiCursor {
                        updated_at,
                        interval_id,
                    })
                });
            drop(app_state);
            let checkpoint_value = match cursor {
                Ok(cursor) => AutoSyncCheckpoint { version: 1, cursor },
                Err(_) => {
                    tracing::warn!(
                        "Graphiti auto-sync could not establish a local no-backfill cursor"
                    );
                    if !wait_while_state_alive(&state, settings.sync_interval).await {
                        return;
                    }
                    continue;
                }
            };
            if write_checkpoint(&checkpoint_path, &checkpoint_value).is_err() {
                tracing::warn!(
                    "Graphiti auto-sync could not persist its local checkpoint; worker stopped"
                );
                return;
            }
            checkpoint = Some(checkpoint_value);
            continue;
        }
        let checkpoint = checkpoint.as_mut().expect("checkpoint initialized");
        let mut had_failure = false;
        for _ in 0..AUTO_SYNC_PER_CYCLE {
            let current_settings = GraphitiSettings::load(&data_dir);
            if !current_settings.auto_sync_enabled
                || current_settings.endpoint.as_ref() != Some(&base_url)
            {
                break;
            }
            let Some(app_state) = state.upgrade() else {
                return;
            };
            let cursor = checkpoint
                .cursor
                .as_ref()
                .map(|cursor| (cursor.updated_at.as_str(), cursor.interval_id));
            let exports = app_state
                .db
                .activity_summary_exports_after(cursor, AUTO_SYNC_PAGE_SIZE)
                .await;
            drop(app_state);
            let exports = match exports {
                Ok(exports) => exports,
                Err(_) => {
                    tracing::warn!("Graphiti auto-sync local summary read failed");
                    had_failure = true;
                    break;
                }
            };
            let Some((interval, updated_at, evidence_refs, source_refs)) =
                exports.into_iter().next()
            else {
                break;
            };
            let next_cursor = GraphitiCursor {
                updated_at: updated_at.clone(),
                interval_id: interval.id,
            };
            let export = SummaryExport {
                interval,
                evidence_refs,
                source_refs,
            };
            let Some(episode) = build_episode(&export) else {
                checkpoint.cursor = Some(next_cursor);
                if write_checkpoint(&checkpoint_path, &checkpoint).is_err() {
                    tracing::warn!("Graphiti auto-sync checkpoint write failed; worker stopped");
                    return;
                }
                continue;
            };
            let Ok(endpoint) = base_url.join("v1/episodes:batch") else {
                return;
            };
            let request_client = client.clone();
            let request = async move {
                let response = request_client
                    .post(endpoint)
                    .json(&[episode])
                    .send()
                    .await
                    .map_err(|_| ())?
                    .error_for_status()
                    .map_err(|_| ())?;
                let body = response.json::<IngestResponse>().await.map_err(|_| ())?;
                Ok::<bool, ()>(body.results.into_iter().any(|result| {
                    result.episode_id == format!("screenpipe:activity:{}", export.interval.id)
                        && matches!(result.status.as_str(), "inserted" | "updated" | "noop")
                }))
            };
            let accepted = match await_while_state_alive(&state, request).await {
                Some(Ok(accepted)) => accepted,
                Some(Err(())) => false,
                None => return,
            };
            if !accepted {
                tracing::warn!(
                    "Graphiti auto-sync request failed; the local summary remains queued for retry"
                );
                had_failure = true;
                break;
            }
            checkpoint.cursor = Some(next_cursor);
            if write_checkpoint(&checkpoint_path, &checkpoint).is_err() {
                tracing::warn!("Graphiti auto-sync checkpoint write failed; duplicate-safe retry will occur after restart");
                had_failure = true;
                break;
            }
            backoff = AUTO_SYNC_INTERVAL;
        }
        let delay = if had_failure {
            backoff
        } else {
            settings.sync_interval
        };
        if !wait_for_sync_delay(&state, &data_dir, &settings, delay).await {
            return;
        }
        if had_failure {
            backoff = backoff.saturating_mul(2).min(AUTO_SYNC_MAX_BACKOFF);
        } else {
            backoff = AUTO_SYNC_INTERVAL;
        }
    }
}

async fn await_while_state_alive<F, T>(
    state: &Weak<crate::server::AppState>,
    future: F,
) -> Option<T>
where
    F: Future<Output = T>,
{
    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return Some(result),
            _ = sleep(STATE_CHECK_INTERVAL) => {
                if state.upgrade().is_none() {
                    return None;
                }
            }
        }
    }
}

async fn wait_while_state_alive(state: &Weak<crate::server::AppState>, duration: Duration) -> bool {
    let mut remaining = duration;
    while !remaining.is_zero() {
        if state.upgrade().is_none() {
            return false;
        }
        let interval = remaining.min(STATE_CHECK_INTERVAL);
        sleep(interval).await;
        remaining = remaining.saturating_sub(interval);
    }
    state.upgrade().is_some()
}

async fn wait_for_sync_delay(
    state: &Weak<crate::server::AppState>,
    data_dir: &Path,
    expected: &GraphitiSettings,
    duration: Duration,
) -> bool {
    let mut remaining = duration;
    while !remaining.is_zero() {
        if state.upgrade().is_none() {
            return false;
        }
        let current = GraphitiSettings::load(data_dir);
        if current.auto_sync_enabled != expected.auto_sync_enabled
            || current.endpoint != expected.endpoint
            || current.sync_interval != expected.sync_interval
        {
            return true;
        }
        let interval = remaining.min(STATE_CHECK_INTERVAL);
        sleep(interval).await;
        remaining = remaining.saturating_sub(interval);
    }
    state.upgrade().is_some()
}

fn load_checkpoint(path: &Path) -> io::Result<Option<AutoSyncCheckpoint>> {
    let contents = match fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&contents)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn write_checkpoint(path: &Path, checkpoint: &AutoSyncCheckpoint) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(CHECKPOINT_FILE);
    let temp_path = path.with_file_name(format!("{file_name}.{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp_path)?;
    serde_json::to_writer(&mut file, checkpoint)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(temp_path, path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_url_validation_fails_closed() {
        assert!(validate_adapter_url("https://graphiti.example.ts.net", false).is_some());
        assert!(validate_adapter_url("http://127.0.0.1:18765", true).is_some());
        for invalid in [
            "http://graphiti.example.ts.net",
            "https://example.com",
            "https://user:secret@graphiti.example.ts.net",
            "https://graphiti.example.ts.net/?token=secret",
            "https://graphiti.example.ts.net/path",
        ] {
            assert!(
                validate_adapter_url(invalid, false).is_none(),
                "accepted {invalid}"
            );
        }
        assert!(validate_adapter_url("http://127.0.0.1:18765", false).is_none());
    }

    #[test]
    fn checkpoint_round_trips_atomically_with_restricted_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CHECKPOINT_FILE);
        let state = AutoSyncCheckpoint {
            version: 1,
            cursor: Some(GraphitiCursor {
                updated_at: "2026-09-26T12:00:00.000Z".into(),
                interval_id: 42,
            }),
        };
        write_checkpoint(&path, &state).unwrap();
        assert_eq!(load_checkpoint(&path).unwrap(), Some(state));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn auto_sync_payload_contains_only_summary_and_cited_metadata() {
        let export = SummaryExport {
            interval: ActivityIntervalRecord {
                id: 42,
                task_id: 1,
                parent_task_id: None,
                kind: "task".into(),
                title: "Synthetic task".into(),
                parent_title: None,
                app_name: Some("Browser".into()),
                start_at: "2026-09-22T10:00:00Z".into(),
                end_at: "2026-09-22T10:01:00Z".into(),
                state: "final".into(),
                confidence: 0.9,
                producer: "deterministic-v2".into(),
                evidence_count: 99,
                actions: Vec::new(),
                evidence: Vec::new(),
                summary: Some("Synthetic summary".into()),
                keywords: Some(vec!["safe".into()]),
                summary_band: Some("short".into()),
                retention: Vec::new(),
            },
            evidence_refs: vec![ActivitySummaryEvidenceRef {
                source_type: "frame".into(),
                source_id: 17,
            }],
            source_refs: vec![ActivityEvidenceRecord {
                source_type: "frame".into(),
                source_id: 17,
                occurred_at: "2026-09-22T10:00:30Z".into(),
                frame_id: Some(17),
                app_name: Some("Browser".into()),
                window_title: Some("Issue".into()),
                browser_url: Some("https://user:pass@example.com/issue?token=x#frag".into()),
            }],
        };
        let episode = build_episode(&export).expect("valid synthetic summary");
        let payload = serde_json::to_value(&episode).unwrap();
        assert_eq!(
            payload["source_refs"][0]["browser_url"],
            "https://example.com/issue"
        );
        assert!(payload.get("evidence").is_none());
        assert!(payload.get("frame").is_none());
        assert!(payload.get("ocr").is_none());
        assert!(payload.get("audio").is_none());
        assert_eq!(payload["episode_body"]["summary"], "Synthetic summary");
        assert_eq!(payload["episode_body"]["status"], "summarized");
        assert_eq!(
            payload["payload_hash"],
            "sha256:0b604f2ef0fa9580e8395584251aa54ede626a472956b47e2ec2336fde28812a"
        );
    }

    #[tokio::test]
    async fn graphiti_search_uses_bounded_mock_transport_and_forwards_time_range() {
        use axum::{extract::State, routing::post, Json, Router};
        use serde_json::{json, Value};

        let captured = Arc::new(Mutex::new(None));
        let app =
            Router::new()
                .route(
                    "/v1/search",
                    post(
                        |State(captured): State<Arc<Mutex<Option<Value>>>>,
                         Json(body): Json<Value>| async move {
                            *captured.lock().unwrap() = Some(body);
                            Json(json!({
                                "hits": [{
                                    "uuid": "synthetic-fact",
                                    "fact": "synthetic fact",
                                    "score": 0.75,
                                    "reference_time": "2026-09-22T10:00:00Z",
                                    "source_refs": [{
                                        "source_type": "frame",
                                        "source_id": 17,
                                        "occurred_at": "2026-09-22T10:00:00Z"
                                    }]
                                }]
                            }))
                        },
                    ),
                )
                .with_state(captured.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let settings = GraphitiSettings {
            auto_sync_enabled: false,
            search_enabled: true,
            endpoint: Some(endpoint),
            sync_interval: Duration::from_secs(30),
        };
        let client = GraphitiClient {
            client: Some(Client::builder().redirect(Policy::none()).build().unwrap()),
        };
        let start = "2026-09-22T09:00:00Z".parse().unwrap();
        let end = "2026-09-22T11:00:00Z".parse().unwrap();
        let hits = client
            .search(&settings, "synthetic query", 25, Some(start), Some(end))
            .await
            .unwrap();
        server.abort();

        assert_eq!(hits[0].uuid, "synthetic-fact");
        let request = captured.lock().unwrap().clone().unwrap();
        assert_eq!(request["limit"], 20);
        assert_eq!(request["query"], "synthetic query");
        assert_eq!(request["start_time"], "2026-09-22T09:00:00Z");
        assert_eq!(request["end_time"], "2026-09-22T11:00:00Z");
    }

    #[tokio::test]
    async fn graphiti_search_disabled_fails_without_network_access() {
        let settings = GraphitiSettings::disabled();
        let client = GraphitiClient { client: None };
        assert!(client
            .search(&settings, "synthetic", 10, None, None)
            .await
            .is_err());
    }

    #[test]
    fn persisted_settings_are_reloaded_and_invalid_settings_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        let disabled = serde_json::json!({
            "adapter_url": "https://graphiti.example.ts.net",
            "allow_http_localhost": false,
            "auto_sync_enabled": false,
            "search_enabled": false,
            "sync_interval_seconds": 90
        });
        fs::write(&path, serde_json::to_vec(&disabled).unwrap()).unwrap();
        let first = GraphitiSettings::load(dir.path());
        assert!(!first.search_enabled);
        assert_eq!(first.sync_interval, Duration::from_secs(90));

        let enabled = serde_json::json!({
            "adapter_url": "https://graphiti.example.ts.net",
            "allow_http_localhost": false,
            "auto_sync_enabled": true,
            "search_enabled": true,
            "sync_interval_seconds": 120
        });
        fs::write(&path, serde_json::to_vec(&enabled).unwrap()).unwrap();
        let second = GraphitiSettings::load(dir.path());
        assert!(second.search_enabled);
        assert!(second.auto_sync_enabled);
        assert_eq!(second.sync_interval, Duration::from_secs(120));

        fs::write(&path, b"not-json").unwrap();
        let invalid = GraphitiSettings::load(dir.path());
        assert!(!invalid.search_enabled);
        assert!(!invalid.auto_sync_enabled);
    }

    #[test]
    fn redacts_url_credentials_query_and_fragment() {
        assert_eq!(
            redact_url("https://user:pass@example.com/path?token=x#frag").as_deref(),
            Some("https://example.com/path")
        );
        assert!(redact_url("file:///private/frame.jpg").is_none());
    }
}
