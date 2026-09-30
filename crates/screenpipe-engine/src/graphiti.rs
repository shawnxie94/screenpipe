// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Optional Graphiti transport. This module only runs when explicitly opted in;
//! capture and SQLite write paths never wait for these network requests.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use reqwest::{redirect::Policy, Client, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    future::Future,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::Duration,
};
use tokio::{sync::Notify, time::sleep};

const AUTO_SYNC_PER_CYCLE: usize = 3;
const MAX_ACTIVITY_HISTORY_BATCH: usize = 64;
const MAX_AUTO_SYNC_QUEUE: usize = 1_000;
const MAX_ACTIVITY_HISTORY_EVIDENCE: usize = 3;
const MAX_ACTIVITY_HISTORY_INPUT_EVIDENCE: usize = 64;
const AUTO_SYNC_INTERVAL: Duration = Duration::from_secs(30);
const AUTO_SYNC_MAX_BACKOFF: Duration = Duration::from_secs(15 * 60);
const AUTO_SYNC_HTTP_TIMEOUT: Duration = Duration::from_secs(310);
const SEARCH_HTTP_TIMEOUT: Duration = Duration::from_secs(2);
const STATE_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const AUTO_SYNC_QUEUE_FILE: &str = "graphiti-auto-sync-queue.json";
const AUTO_SYNC_STATUS_FILE: &str = "graphiti-auto-sync-status.json";
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
static AUTO_SYNC_WORKERS: OnceLock<Mutex<HashMap<PathBuf, Arc<Notify>>>> = OnceLock::new();

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
    let workers = AUTO_SYNC_WORKERS.get_or_init(|| Mutex::new(HashMap::new()));
    let notification = {
        let mut workers = workers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if workers.contains_key(&key) {
            return;
        }
        let notification = Arc::new(Notify::new());
        workers.insert(key.clone(), notification.clone());
        notification
    };
    let state = Arc::downgrade(state);
    tokio::spawn(async move {
        auto_sync_loop(state, data_dir, client, notification).await;
        if let Some(workers) = AUTO_SYNC_WORKERS.get() {
            workers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&key);
        }
    });
}

fn wake_auto_sync_if_running(data_dir: &Path) {
    let Some(workers) = AUTO_SYNC_WORKERS.get() else {
        return;
    };
    let notification = workers
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(data_dir)
        .cloned();
    if let Some(notification) = notification {
        notification.notify_one();
    }
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
#[serde(rename_all = "snake_case")]
enum AutoSyncOutcome {
    Success,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct AutoSyncStatus {
    version: u8,
    last_attempt_at: Option<String>,
    status: Option<AutoSyncOutcome>,
    delivered_count: u64,
}

impl Default for AutoSyncStatus {
    fn default() -> Self {
        Self {
            version: 1,
            last_attempt_at: None,
            status: None,
            delivered_count: 0,
        }
    }
}

fn completed_sync_status(delivered_count: u64, had_failure: bool) -> AutoSyncStatus {
    let status = match (had_failure, delivered_count > 0) {
        (false, _) => AutoSyncOutcome::Success,
        (true, true) => AutoSyncOutcome::Partial,
        (true, false) => AutoSyncOutcome::Failed,
    };
    AutoSyncStatus {
        version: 1,
        last_attempt_at: Some(Utc::now().to_rfc3339()),
        status: Some(status),
        delivered_count,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActivityHistorySyncBatch {
    entries: Vec<ActivityHistorySyncEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActivityHistorySyncEntry {
    id: String,
    kind: String,
    meeting_id: Option<i64>,
    start_at: String,
    end_at: String,
    title: String,
    summary: String,
    confidence: f64,
    activity_type: Option<String>,
    project_refs: Vec<String>,
    outcomes: Vec<ActivityHistorySyncOutcome>,
    semantic_status: Option<String>,
    evidence: Vec<ActivityHistorySyncEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ActivityHistorySyncOutcome {
    #[serde(rename = "type", alias = "outcome_type")]
    outcome_type: String,
    status: String,
    confidence: f64,
    provenance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActivityHistorySyncEvidence {
    kind: String,
    at: String,
    source_type: Option<String>,
    source_id: Option<i64>,
    occurred_at: Option<String>,
    frame_id: Option<i64>,
    meeting_id: Option<i64>,
    app_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AutoSyncQueue {
    version: u8,
    episodes: Vec<GraphitiEpisode>,
}

impl Default for AutoSyncQueue {
    fn default() -> Self {
        Self {
            version: 1,
            episodes: Vec::new(),
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct GraphitiEpisode {
    episode_id: String,
    name: String,
    reference_time: String,
    source_description: String,
    episode_body: GraphitiEpisodeBody,
    source_refs: Vec<GraphitiEpisodeSourceRef>,
    privacy: String,
    payload_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct GraphitiEpisodeBody {
    activity_id: String,
    kind: String,
    meeting_id: Option<i64>,
    activity_type: String,
    start_at: String,
    end_at: String,
    title: String,
    summary: String,
    project_refs: Vec<String>,
    outcomes: Vec<ActivityHistorySyncOutcome>,
    confidence: f64,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct GraphitiEpisodeSourceRef {
    source_type: String,
    source_id: i64,
    occurred_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_name: Option<String>,
}

fn build_episode(entry: &ActivityHistorySyncEntry) -> Option<GraphitiEpisode> {
    let start = DateTime::parse_from_rfc3339(&entry.start_at)
        .ok()?
        .with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339(&entry.end_at)
        .ok()?
        .with_timezone(&Utc);
    if start >= end
        || entry.id.trim().is_empty()
        || entry.id.len() > 160
        || !matches!(entry.kind.as_str(), "work" | "meeting")
        || entry.title.trim().is_empty()
        || entry.summary.trim().is_empty()
        || !entry.confidence.is_finite()
        || !(0.0..=1.0).contains(&entry.confidence)
        || !matches!(
            entry.semantic_status.as_deref(),
            Some("summarized" | "inferred")
        )
        || (entry.kind == "meeting" && entry.meeting_id.is_none())
        || entry.evidence.is_empty()
        || entry.evidence.len() > MAX_ACTIVITY_HISTORY_INPUT_EVIDENCE
        || entry.project_refs.len() > 32
        || entry.outcomes.len() > 12
    {
        return None;
    }

    let activity_type = entry.activity_type.as_deref()?.trim();
    if !matches!(
        activity_type,
        "meeting"
            | "research"
            | "implementation"
            | "planning"
            | "communication"
            | "learning"
            | "administrative"
            | "unknown"
    ) {
        return None;
    }
    let mut cited = HashSet::with_capacity(MAX_ACTIVITY_HISTORY_EVIDENCE);
    let mut source_refs = Vec::with_capacity(MAX_ACTIVITY_HISTORY_EVIDENCE);
    for evidence in &entry.evidence {
        let source_type = evidence
            .source_type
            .as_deref()
            .unwrap_or_else(|| match evidence.kind.as_str() {
                "screen" => "frame",
                "audio" => "audio",
                "meeting" => "meeting",
                _ => "",
            })
            .trim();
        let Some(source_id) = evidence
            .source_id
            .or(evidence.frame_id)
            .or(evidence.meeting_id)
            .filter(|id| *id > 0)
        else {
            continue;
        };
        let occurred_at = evidence.occurred_at.as_deref().unwrap_or(&evidence.at);
        let Ok(occurred) = DateTime::parse_from_rfc3339(occurred_at) else {
            continue;
        };
        let occurred = occurred.with_timezone(&Utc);
        if !matches!(
            source_type,
            "frame" | "audio" | "meeting" | "ui_event" | "parsed"
        ) || occurred < start
            || occurred > end
            || !cited.insert((source_type.to_string(), source_id))
        {
            continue;
        }
        source_refs.push(GraphitiEpisodeSourceRef {
            source_type: source_type.to_string(),
            source_id,
            occurred_at: occurred.to_rfc3339(),
            frame_id: evidence.frame_id,
            app_name: evidence
                .app_name
                .as_deref()
                .map(|name| name.chars().take(160).collect()),
        });
        if source_refs.len() == MAX_ACTIVITY_HISTORY_EVIDENCE {
            break;
        }
    }
    if source_refs.is_empty() {
        return None;
    }

    let project_refs: Vec<String> = entry
        .project_refs
        .iter()
        .map(|project| project.trim())
        .filter(|project| !project.is_empty() && project.len() <= 120)
        .map(str::to_string)
        .collect();
    if project_refs.len() != entry.project_refs.len() {
        return None;
    }
    let outcomes = entry
        .outcomes
        .iter()
        .map(|outcome| {
            if !matches!(
                outcome.outcome_type.as_str(),
                "decision" | "deliverable" | "commitment" | "blocker" | "next_step" | "unknown"
            ) || !matches!(
                outcome.status.as_str(),
                "observed" | "summarized" | "inferred" | "confirmed"
            ) || !outcome.confidence.is_finite()
                || !(0.0..=1.0).contains(&outcome.confidence)
                || outcome.provenance.trim().is_empty()
                || outcome.provenance.len() > 200
            {
                None
            } else {
                Some(outcome.clone())
            }
        })
        .collect::<Option<Vec<_>>>()?;
    let body = GraphitiEpisodeBody {
        activity_id: entry.id.clone(),
        kind: entry.kind.clone(),
        meeting_id: entry.meeting_id,
        activity_type: activity_type.to_string(),
        start_at: start.to_rfc3339(),
        end_at: end.to_rfc3339(),
        title: entry.title.trim().chars().take(240).collect(),
        summary: entry.summary.trim().chars().take(8_000).collect(),
        project_refs,
        outcomes,
        confidence: entry.confidence,
        status: entry.semantic_status.clone()?,
    };
    let unsigned = serde_json::to_vec(&(&body, &source_refs, "private")).ok()?;
    let payload_hash = format!("sha256:{}", hex::encode(Sha256::digest(unsigned)));
    Some(GraphitiEpisode {
        episode_id: format!("screenpipe:activity:{}", entry.id),
        name: body.title.clone(),
        reference_time: body.end_at.clone(),
        source_description: "screenpipe.activity_history".to_string(),
        episode_body: body,
        source_refs,
        privacy: "private".to_string(),
        payload_hash,
    })
}

#[derive(Debug, Serialize)]
struct ActivityHistoryEnqueueResponse {
    enabled: bool,
    queued_count: usize,
    skipped_count: usize,
}

static AUTO_SYNC_QUEUE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) async fn enqueue_activity_history(
    State(state): State<Arc<crate::server::AppState>>,
    Json(batch): Json<ActivityHistorySyncBatch>,
) -> axum::response::Response {
    if batch.entries.len() > MAX_ACTIVITY_HISTORY_BATCH {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let settings = GraphitiSettings::load(&state.screenpipe_dir);
    if !settings.auto_sync_enabled || settings.endpoint.is_none() {
        return Json(ActivityHistoryEnqueueResponse {
            enabled: false,
            queued_count: 0,
            skipped_count: batch.entries.len(),
        })
        .into_response();
    }

    let data_dir = state.screenpipe_dir.clone();
    match enqueue_activity_history_entries(&data_dir, batch.entries) {
        Ok((queued_count, skipped_count)) => {
            if skipped_count > 0 {
                tracing::info!(
                    skipped_count,
                    "Graphiti skipped ineligible Activity History entries"
                );
            }
            wake_auto_sync_if_running(&data_dir);
            ensure_auto_sync(&state);
            Json(ActivityHistoryEnqueueResponse {
                enabled: true,
                queued_count,
                skipped_count,
            })
            .into_response()
        }
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "graphiti_sync_queue_unavailable" })),
        )
            .into_response(),
    }
}

fn enqueue_activity_history_entries(
    data_dir: &Path,
    entries: Vec<ActivityHistorySyncEntry>,
) -> io::Result<(usize, usize)> {
    let lock = AUTO_SYNC_QUEUE_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let episodes: Vec<_> = entries.iter().filter_map(build_episode).collect();
    let skipped_count = entries.len().saturating_sub(episodes.len());
    let path = data_dir.join(AUTO_SYNC_QUEUE_FILE);
    let mut queue = load_auto_sync_queue(&path)?;

    let mut next = queue.episodes.clone();
    for episode in &episodes {
        if let Some(existing) = next
            .iter_mut()
            .find(|queued| queued.episode_id == episode.episode_id)
        {
            *existing = episode.clone();
        } else {
            next.push(episode.clone());
        }
    }
    if next.len() > MAX_AUTO_SYNC_QUEUE {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "Graphiti auto-sync queue is full",
        ));
    }
    queue.episodes = next;
    write_private_json_atomic(&path, AUTO_SYNC_QUEUE_FILE, &queue)?;
    Ok((episodes.len(), skipped_count))
}

fn load_auto_sync_queue(path: &Path) -> io::Result<AutoSyncQueue> {
    let contents = match fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(AutoSyncQueue::default());
        }
        Err(error) => return Err(error),
    };
    let queue: AutoSyncQueue = serde_json::from_slice(&contents)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if queue.version != 1 || queue.episodes.len() > MAX_AUTO_SYNC_QUEUE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported or oversized Graphiti auto-sync queue",
        ));
    }
    Ok(queue)
}

fn acknowledge_queued_episode(path: &Path, episode_id: &str, payload_hash: &str) -> io::Result<()> {
    let lock = AUTO_SYNC_QUEUE_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut queue = load_auto_sync_queue(path)?;
    queue
        .episodes
        .retain(|episode| episode.episode_id != episode_id || episode.payload_hash != payload_hash);
    write_private_json_atomic(path, AUTO_SYNC_QUEUE_FILE, &queue)
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

#[derive(Debug, PartialEq, Eq)]
enum DeliveryFailure {
    EndpointConstruction,
    Transport,
    HttpStatus(u16),
    InvalidResponse,
    UnexpectedAcknowledgement,
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

async fn auto_sync_loop(
    state: Weak<crate::server::AppState>,
    data_dir: PathBuf,
    client: Client,
    notification: Arc<Notify>,
) {
    let queue_path = data_dir.join(AUTO_SYNC_QUEUE_FILE);
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
        let mut had_failure = false;
        let mut delivered_count = 0;
        for _ in 0..AUTO_SYNC_PER_CYCLE {
            let current_settings = GraphitiSettings::load(&data_dir);
            if !current_settings.auto_sync_enabled
                || current_settings.endpoint.as_ref() != Some(&base_url)
            {
                break;
            }
            let queue = match load_auto_sync_queue(&queue_path) {
                Ok(queue) => queue,
                Err(_) => {
                    tracing::warn!("Graphiti auto-sync queue is unreadable; no data was sent");
                    had_failure = true;
                    break;
                }
            };
            let Some(episode) = queue.episodes.first().cloned() else {
                break;
            };
            let request_client = client.clone();
            let request_base = base_url.clone();
            let request_episode = episode.clone();
            let request = async move {
                deliver_episode(&request_client, &request_base, &request_episode).await
            };
            let failure = match await_while_state_alive(&state, request).await {
                Some(Ok(())) => None,
                Some(Err(failure)) => Some(failure),
                None => return,
            };
            if let Some(failure) = failure {
                tracing::warn!(
                    ?failure,
                    "Graphiti auto-sync delivery failed; queued Activity History remains for retry"
                );
                had_failure = true;
                break;
            }
            if acknowledge_queued_episode(&queue_path, &episode.episode_id, &episode.payload_hash)
                .is_err()
            {
                tracing::warn!("Graphiti auto-sync could not acknowledge a delivered queue item; duplicate-safe retry will occur");
                had_failure = true;
                break;
            }
            delivered_count += 1;
            backoff = AUTO_SYNC_INTERVAL;
        }
        let status = completed_sync_status(delivered_count, had_failure);
        if write_auto_sync_status(&data_dir.join(AUTO_SYNC_STATUS_FILE), &status).is_err() {
            tracing::warn!("Graphiti auto-sync status could not be persisted");
        }
        let delay = if had_failure {
            backoff
        } else {
            settings.sync_interval
        };
        if !wait_for_sync_delay(
            &state,
            &data_dir,
            &settings,
            delay,
            &notification,
            !had_failure,
        )
        .await
        {
            return;
        }
        if had_failure {
            backoff = backoff.saturating_mul(2).min(AUTO_SYNC_MAX_BACKOFF);
        } else {
            backoff = AUTO_SYNC_INTERVAL;
        }
    }
}

async fn deliver_episode(
    client: &Client,
    base_url: &Url,
    episode: &GraphitiEpisode,
) -> Result<(), DeliveryFailure> {
    let endpoint = base_url
        .join("v1/episodes:batch")
        .map_err(|_| DeliveryFailure::EndpointConstruction)?;
    let response = client
        .post(endpoint)
        .json(std::slice::from_ref(episode))
        .send()
        .await
        .map_err(|_| DeliveryFailure::Transport)?;
    let status = response.status();
    if !status.is_success() {
        return Err(DeliveryFailure::HttpStatus(status.as_u16()));
    }
    let body = response
        .json::<IngestResponse>()
        .await
        .map_err(|_| DeliveryFailure::InvalidResponse)?;
    if body.results.into_iter().any(|result| {
        result.episode_id == episode.episode_id
            && matches!(result.status.as_str(), "inserted" | "updated" | "noop")
    }) {
        Ok(())
    } else {
        Err(DeliveryFailure::UnexpectedAcknowledgement)
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

async fn wait_for_auto_sync_notification(
    notification: &Notify,
    queue_path: &Path,
    timeout: Duration,
) -> io::Result<Option<bool>> {
    tokio::select! {
        _ = sleep(timeout) => Ok(None),
        _ = notification.notified() => {
            let queue = load_auto_sync_queue(queue_path)?;
            Ok(Some(!queue.episodes.is_empty()))
        }
    }
}

async fn wait_for_sync_delay(
    state: &Weak<crate::server::AppState>,
    data_dir: &Path,
    expected: &GraphitiSettings,
    duration: Duration,
    notification: &Notify,
    wake_on_enqueue: bool,
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
        if wake_on_enqueue {
            match wait_for_auto_sync_notification(
                notification,
                &data_dir.join(AUTO_SYNC_QUEUE_FILE),
                interval,
            )
            .await
            {
                Ok(Some(true)) | Err(_) => return true,
                Ok(Some(false)) => continue,
                Ok(None) => {}
            }
        } else {
            sleep(interval).await;
        }
        remaining = remaining.saturating_sub(interval);
    }
    state.upgrade().is_some()
}

#[cfg(test)]
fn load_auto_sync_status(path: &Path) -> io::Result<AutoSyncStatus> {
    let contents = match fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(AutoSyncStatus::default());
        }
        Err(error) => return Err(error),
    };
    let status: AutoSyncStatus = serde_json::from_slice(&contents)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if status.version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported Graphiti sync status version",
        ));
    }
    Ok(status)
}

fn write_auto_sync_status(path: &Path, status: &AutoSyncStatus) -> io::Result<()> {
    write_private_json_atomic(path, AUTO_SYNC_STATUS_FILE, status)
}

fn write_private_json_atomic<T: Serialize>(
    path: &Path,
    fallback_name: &str,
    value: &T,
) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(fallback_name);
    let temp_path = path.with_file_name(format!("{file_name}.{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp_path)?;
    serde_json::to_writer(&mut file, value)
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

    fn sample_activity_history_entry() -> ActivityHistorySyncEntry {
        ActivityHistorySyncEntry {
            id: "history-synthetic-1".into(),
            kind: "work".into(),
            meeting_id: None,
            start_at: "2026-09-22T10:00:00Z".into(),
            end_at: "2026-09-22T10:01:00Z".into(),
            title: "Synthetic research".into(),
            summary: "A bounded synthetic activity summary".into(),
            confidence: 0.9,
            activity_type: Some("research".into()),
            project_refs: vec!["synthetic-project".into()],
            outcomes: vec![ActivityHistorySyncOutcome {
                outcome_type: "decision".into(),
                status: "inferred".into(),
                confidence: 0.8,
                provenance: "synthetic evidence".into(),
            }],
            semantic_status: Some("inferred".into()),
            evidence: vec![ActivityHistorySyncEvidence {
                kind: "screen".into(),
                at: "2026-09-22T10:00:30Z".into(),
                source_type: Some("frame".into()),
                source_id: Some(17),
                occurred_at: Some("2026-09-22T10:00:30Z".into()),
                frame_id: Some(17),
                meeting_id: None,
                app_name: Some("Synthetic Browser".into()),
            }],
        }
    }

    #[test]
    fn activity_history_episode_contains_only_summary_and_cited_metadata() {
        let entry = sample_activity_history_entry();
        let episode = build_episode(&entry).expect("valid synthetic Activity History");
        let payload = serde_json::to_value(&episode).unwrap();
        assert_eq!(
            episode.episode_id,
            "screenpipe:activity:history-synthetic-1"
        );
        assert_eq!(episode.source_description, "screenpipe.activity_history");
        assert_eq!(payload["episode_body"]["summary"], entry.summary);
        assert_eq!(payload["episode_body"]["activity_type"], "research");
        assert_eq!(payload["episode_body"]["outcomes"][0]["type"], "decision");
        assert_eq!(payload["source_refs"][0]["source_id"], 17);
        assert!(payload["source_refs"][0].get("label").is_none());
        assert!(payload.get("evidence").is_none());
        assert!(payload.get("ocr").is_none());
        assert!(payload.get("audio").is_none());
    }

    #[test]
    fn activity_history_episode_rejects_ineligible_structure() {
        let mut entry = sample_activity_history_entry();
        entry.evidence.clear();
        assert!(build_episode(&entry).is_none());

        let mut entry = sample_activity_history_entry();
        entry.semantic_status = Some("rejected".into());
        assert!(build_episode(&entry).is_none());

        let mut entry = sample_activity_history_entry();
        entry.evidence[0].occurred_at = Some("not-a-time".into());
        assert!(build_episode(&entry).is_none());

        let mut entry = sample_activity_history_entry();
        entry.evidence.push(entry.evidence[0].clone());
        let episode = build_episode(&entry).expect("duplicate refs collapse safely");
        assert_eq!(episode.source_refs.len(), 1);

        let mut entry = sample_activity_history_entry();
        for source_id in 18..22 {
            let mut evidence = entry.evidence[0].clone();
            evidence.source_id = Some(source_id);
            evidence.frame_id = Some(source_id);
            entry.evidence.push(evidence);
        }
        let episode = build_episode(&entry).expect("valid refs are bounded");
        assert_eq!(episode.source_refs.len(), MAX_ACTIVITY_HISTORY_EVIDENCE);
    }

    #[test]
    fn auto_sync_queue_is_atomic_private_and_never_backfills_stored_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(AUTO_SYNC_QUEUE_FILE);
        assert!(load_auto_sync_queue(&path).unwrap().episodes.is_empty());
        fs::write(
            dir.path().join("graphiti-auto-sync-state.json"),
            b"legacy cursor",
        )
        .unwrap();

        let (queued, skipped) =
            enqueue_activity_history_entries(dir.path(), vec![sample_activity_history_entry()])
                .unwrap();
        assert_eq!((queued, skipped), (1, 0));
        let queue = load_auto_sync_queue(&path).unwrap();
        assert_eq!(queue.episodes.len(), 1);
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
    fn auto_sync_queue_upserts_by_stable_activity_id() {
        let dir = tempfile::tempdir().unwrap();
        let first = sample_activity_history_entry();
        enqueue_activity_history_entries(dir.path(), vec![first.clone()]).unwrap();
        let mut revised = first;
        revised.summary = "Updated synthetic summary".into();
        enqueue_activity_history_entries(dir.path(), vec![revised]).unwrap();

        let queue = load_auto_sync_queue(&dir.path().join(AUTO_SYNC_QUEUE_FILE)).unwrap();
        assert_eq!(queue.episodes.len(), 1);
        assert_eq!(
            queue.episodes[0].episode_body.summary,
            "Updated synthetic summary"
        );
    }

    #[tokio::test]
    async fn auto_sync_notification_only_wakes_for_pending_queue_work() {
        let dir = tempfile::tempdir().unwrap();
        let queue_path = dir.path().join(AUTO_SYNC_QUEUE_FILE);
        let notification = Notify::new();

        notification.notify_one();
        assert_eq!(
            wait_for_auto_sync_notification(&notification, &queue_path, Duration::from_secs(1),)
                .await
                .unwrap(),
            Some(false)
        );

        enqueue_activity_history_entries(dir.path(), vec![sample_activity_history_entry()])
            .unwrap();
        notification.notify_one();
        assert_eq!(
            wait_for_auto_sync_notification(&notification, &queue_path, Duration::from_secs(1),)
                .await
                .unwrap(),
            Some(true)
        );
    }

    #[test]
    fn auto_sync_status_is_atomic_private_and_contains_only_summary_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(AUTO_SYNC_STATUS_FILE);
        let status = completed_sync_status(2, true);
        assert_eq!(status.status, Some(AutoSyncOutcome::Partial));
        assert_eq!(status.delivered_count, 2);
        assert!(DateTime::parse_from_rfc3339(status.last_attempt_at.as_deref().unwrap()).is_ok());

        write_auto_sync_status(&path, &status).unwrap();
        assert_eq!(load_auto_sync_status(&path).unwrap(), status);
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 4);
        assert!(value.get("summary").is_none());
        assert!(value.get("source_refs").is_none());
        assert!(value.get("error").is_none());
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
    fn auto_sync_status_distinguishes_no_attempt_success_and_failure() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load_auto_sync_status(&dir.path().join(AUTO_SYNC_STATUS_FILE)).unwrap(),
            AutoSyncStatus::default()
        );
        assert_eq!(
            completed_sync_status(0, false).status,
            Some(AutoSyncOutcome::Success)
        );
        assert_eq!(
            completed_sync_status(0, true).status,
            Some(AutoSyncOutcome::Failed)
        );
        assert_eq!(
            completed_sync_status(1, true).status,
            Some(AutoSyncOutcome::Partial)
        );
    }

    #[tokio::test]
    async fn delivery_failures_expose_only_safe_categories() {
        use axum::{http::StatusCode, routing::post, Json, Router};
        use serde_json::json;

        let episode = build_episode(&sample_activity_history_entry()).unwrap();

        let app = Router::new().route(
            "/v1/episodes:batch",
            post(|| async { (StatusCode::SERVICE_UNAVAILABLE, "private adapter detail") }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Client::builder().redirect(Policy::none()).build().unwrap();
        assert_eq!(
            deliver_episode(&client, &endpoint, &episode).await,
            Err(DeliveryFailure::HttpStatus(503))
        );
        server.abort();

        let app = Router::new().route(
            "/v1/episodes:batch",
            post(|| async {
                Json(json!({
                    "results": [{"episode_id": "unmatched-synthetic-id", "status": "inserted"}]
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        assert_eq!(
            deliver_episode(&client, &endpoint, &episode).await,
            Err(DeliveryFailure::UnexpectedAcknowledgement)
        );
        server.abort();
    }

    #[tokio::test]
    async fn activity_history_episode_delivery_accepts_only_matching_adapter_ack() {
        use axum::{routing::post, Json, Router};
        use serde_json::{json, Value};

        let captured = Arc::new(Mutex::new(None));
        let captured_for_route = captured.clone();
        let app = Router::new().route(
            "/v1/episodes:batch",
            post(move |Json(body): Json<Value>| {
                let captured = captured_for_route.clone();
                async move {
                    *captured.lock().unwrap() = Some(body);
                    Json(json!({
                        "results": [{
                            "episode_id": "screenpipe:activity:history-synthetic-1",
                            "status": "inserted"
                        }]
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Client::builder().redirect(Policy::none()).build().unwrap();
        let episode = build_episode(&sample_activity_history_entry()).unwrap();

        assert_eq!(deliver_episode(&client, &endpoint, &episode).await, Ok(()));
        server.abort();
        let body = captured.lock().unwrap().clone().unwrap();
        assert_eq!(body[0]["source_description"], "screenpipe.activity_history");
        assert_eq!(
            body[0]["episode_body"]["activity_id"],
            "history-synthetic-1"
        );
        assert!(body[0]["source_refs"][0].get("label").is_none());
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
