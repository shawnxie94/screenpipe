// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Desktop-side Knowledge runtime: the Pi executor following the user's preset
//! selection and the KnowledgeWorker lifecycle.
//!
//! Binding contract (2026-09-08 user-directed change of TRD §7.2):
//! - the knowledge reuses the「模型与密钥」selection — the default preset, falling
//!   back to the first entry; ACP presets are supported alongside BYOK ones,
//!   and switching the selection switches the runtime with no restart;
//! - ordinary (non-ACP) completions run without tools (`allowed_tools`) and
//!   under knowledge-side token/context budgets;
//! - every execution uses a `__title:` prefixed session id, so Pi does not
//!   persist sessions, plus a dedicated temp directory removed afterwards;
//! - credentials come from the preset store (file-encrypted with the OS
//!   keychain key) and are only referenced, never logged.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use screenpipe_engine::activity_ledger::{self, SegmentationPolicy};
use screenpipe_engine::knowledge::cadence::KnowledgeCadenceConfig;
use screenpipe_engine::knowledge::distill;
use screenpipe_engine::knowledge::executor::{
    KnowledgeModelExecutor, KnowledgeLimits, CompletionRequest, ModelIdentity,
};
use screenpipe_engine::knowledge::types::KnowledgeError;
use screenpipe_engine::knowledge::worker::{self, JobHandlers, WorkerConfig, WorkerHandle};

use crate::pi::{self, AcpAgentConfig, PiBackend, PiProviderConfig, PiState};
use crate::store::{AIProviderType, AIPreset, SettingsStore};

/// Execution profile version; enters extraction/compilation input hashes.
pub const KNOWLEDGE_PROFILE_VERSION: &str = "knowledge-extract-v1";

#[derive(Default)]
pub struct KnowledgeRuntimeState {
    worker: tokio::sync::RwLock<Option<Arc<WorkerHandle>>>,
}

impl KnowledgeRuntimeState {
    pub async fn worker_handle(&self) -> Option<Arc<WorkerHandle>> {
        self.worker.read().await.clone()
    }
}

/// The preset selected in「模型与密钥」: the user's default preset, falling
/// back to the first entry. Switching the selection switches the knowledge's
/// runtime on the next call — no hardcoded preset or model id.
fn resolve_selected_preset(settings: &SettingsStore) -> Result<&AIPreset, String> {
    settings
        .ai_presets
        .iter()
        .find(|p| p.default_preset)
        .or_else(|| settings.ai_presets.first())
        .ok_or_else(|| {
            "模型与密钥中还没有 AI 预设；请先在设置中添加并选择一个预设，再使用知迹的问答与抽取"
                .to_string()
        })
}

/// Binding summary for status/hash records (never the secret itself).
struct PresetBinding {
    preset_id: String,
    model_id: String,
    wire_api: String,
}

fn preset_binding(preset: &AIPreset) -> PresetBinding {
    let is_acp = matches!(preset.provider, AIProviderType::Acp);
    let model_id = if preset.model.trim().is_empty() {
        preset
            .acp_agent
            .as_ref()
            .map(|agent| agent.id.clone())
            .unwrap_or_default()
    } else {
        preset.model.trim().to_string()
    };
    PresetBinding {
        preset_id: preset.id.clone(),
        model_id,
        wire_api: if is_acp { "acp" } else { "openai-completions" }.to_string(),
    }
}

fn provider_name(provider: &AIProviderType) -> String {
    serde_json::to_value(provider)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "custom".to_string())
}

/// Selected preset → Pi provider config, mirroring the chat mapping (ACP
/// presets keep their agent; BYOK presets keep url/model/key). Knowledge-side
/// resource budgets and the tool ban on ordinary completions still apply.
fn resolve_provider_config(
    settings: &SettingsStore,
    task_system_prompt: &str,
) -> Result<(PiProviderConfig, PresetBinding), String> {
    let preset = resolve_selected_preset(settings)?;
    let is_acp = matches!(preset.provider, AIProviderType::Acp);
    if preset.model.trim().is_empty() && !is_acp {
        return Err(format!(
            "AI 预设「{}」还没有配置模型；请在「模型与密钥」中补全后再使用知迹",
            preset.id
        ));
    }
    let acp_agent = if is_acp {
        let agent = preset.acp_agent.as_ref().ok_or_else(|| {
            format!("ACP 预设「{}」缺少 agent 配置；请在「模型与密钥」中补全", preset.id)
        })?;
        Some(AcpAgentConfig {
            id: agent.id.clone(),
            command: agent.command.clone(),
            args: agent.args.clone(),
            env: agent.env.clone(),
            auth_method: None,
            config: agent.config.clone(),
            mode_id: agent.mode_id.clone(),
            approval_mode: agent.approval_mode.clone(),
        })
    } else {
        None
    };
    let binding = preset_binding(preset);
    let config = PiProviderConfig {
        backend: is_acp.then_some(PiBackend::Acp),
        acp_agent,
        provider: provider_name(&preset.provider),
        url: preset.url.clone(),
        model: binding.model_id.clone(),
        api_key: preset.api_key.clone(),
        max_tokens: preset.max_tokens.clamp(2_048, KnowledgeLimits::MAX_OUTPUT_TOKENS as i32),
        max_context_chars: Some(preset.max_context_chars.min(48_000)),
        system_prompt: Some(
            [preset.prompt.trim(), task_system_prompt]
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n"),
        ),
        allowed_tools: if is_acp { None } else { Some(Vec::new()) },
        resume_session_id: None,
        unattended: false,
    };
    Ok((config, binding))
}

/// One ordinary completion through the app's Pi sidecar, using whichever
/// preset the user currently has selected in「模型与密钥」.
async fn run_strict_completion(
    app: &AppHandle,
    request: CompletionRequest,
    cancel: CancellationToken,
) -> Result<String, KnowledgeError> {
    let settings = SettingsStore::get(app)
        .map_err(|e| KnowledgeError::new("settings_unavailable", e.to_string(), true))?
        .ok_or_else(|| KnowledgeError::new("settings_unavailable", "设置不可用", true))?;
    let (config, _binding) = resolve_provider_config(&settings, &request.system)
        .map_err(|message| KnowledgeError::new("model_binding_invalid", message, false))?;

    // Dedicated temp working directory; cleaned up on every exit path.
    let session_dir: PathBuf = screenpipe_core::paths::default_screenpipe_data_dir()
        .join("knowledge")
        .join("sessions")
        .join(uuid::Uuid::new_v4().to_string());
    let _ = std::fs::create_dir_all(&session_dir);

    let session_id = format!("__title:knowledge-{}-{}", request.purpose, uuid::Uuid::new_v4());
    let project_dir = session_dir.to_string_lossy().to_string();
    let state = app.state::<PiState>();
    let mut events = pi::subscribe_internal_agent_events();

    let started = pi::pi_start_inner(
        app.clone(),
        state.inner(),
        &session_id,
        project_dir,
        Some(config),
        None,
    )
    .await
    .map_err(|error| KnowledgeError::new("model_start_failed", error, true))?;
    if !started.running {
        cleanup_session(&session_dir);
        return Err(KnowledgeError::new("model_start_failed", "AI 未启动", true));
    }

    // Cancel watcher: stop the sidecar process when the token fires.
    let cancel_app = app.clone();
    let cancel_state = state.inner().clone();
    let cancel_session = session_id.clone();
    let watcher = tokio::spawn(async move {
        cancel.cancelled().await;
        let mut pool = cancel_state.0.lock().await;
        if let Some(manager) = pool.sessions.get_mut(&cancel_session) {
            manager.stop().await;
        }
        drop(cancel_app);
    });

    let prompt_result = pi::pi_prompt_inner(
        app,
        state.inner(),
        &session_id,
        request.user_prompt,
        None,
        None,
    )
    .await;

    let outcome = match prompt_result {
        Ok(_queue_id) => {
            let collect = async {
                loop {
                    let envelope = match events.recv().await {
                        Ok(envelope) => envelope,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(e) => {
                            break Err(KnowledgeError::new("model_event_error", e.to_string(), true))
                        }
                    };
                    if envelope.session_id != session_id {
                        continue;
                    }
                    match classify_knowledge_event(&envelope.event) {
                        KnowledgeEvent::Complete(text) => break Ok(text),
                        KnowledgeEvent::Fail(message) => {
                            break Err(KnowledgeError::new("model_error", message, true))
                        }
                        KnowledgeEvent::Ignore => {}
                    }
                }
            };
            match tokio::time::timeout(request.timeout, collect).await {
                Ok(result) => result,
                Err(_) => Err(KnowledgeError::new(
                    "model_timeout",
                    format!("模型调用超过 {}s", request.timeout.as_secs()),
                    true,
                )),
            }
        }
        Err(error) => Err(KnowledgeError::new("model_prompt_failed", error, true)),
    };

    watcher.abort();
    {
        let mut pool = state.0.lock().await;
        if let Some(manager) = pool.sessions.get_mut(&session_id) {
            manager.stop().await;
        }
    }
    cleanup_session(&session_dir);
    outcome
}

fn cleanup_session(dir: &PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

enum KnowledgeEvent {
    Complete(String),
    Fail(String),
    Ignore,
}

/// Strict profile: empty completions are a transient failure — the worker
/// owns the retry budget (existing `run_background_pi` retries are NOT
/// stacked on top).
fn classify_knowledge_event(event: &Value) -> KnowledgeEvent {
    match event.get("type").and_then(Value::as_str) {
        Some("agent_end") => {
            if event.get("willRetry").and_then(Value::as_bool) == Some(true) {
                KnowledgeEvent::Ignore
            } else if let Some(error) = knowledge_event_error_text(event) {
                KnowledgeEvent::Fail(error)
            } else if let Some(text) = knowledge_final_assistant_text(event) {
                if text.trim().is_empty() {
                    KnowledgeEvent::Fail("模型返回空结果".to_string())
                } else {
                    KnowledgeEvent::Complete(text)
                }
            } else {
                KnowledgeEvent::Fail("模型返回空结果".to_string())
            }
        }
        Some("error") => KnowledgeEvent::Fail(
            knowledge_event_error_text(event).unwrap_or_else(|| "模型执行失败".to_string()),
        ),
        Some("agent_terminated") => KnowledgeEvent::Fail("Pi 进程被终止".to_string()),
        _ => KnowledgeEvent::Ignore,
    }
}

fn knowledge_event_error_text(event: &Value) -> Option<String> {
    let direct = [
        event.get("errorMessage"),
        event.get("finalError"),
        event.get("message").filter(|m| m.is_string()),
        event.get("message").and_then(|m| m.get("errorMessage")),
        event.get("message").and_then(|m| m.get("error")),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .find(|m| !m.trim().is_empty());
    direct.map(|m| m.trim().to_string())
}

fn knowledge_final_assistant_text(event: &Value) -> Option<String> {
    let assistant = event
        .get("messages")
        .and_then(Value::as_array)?
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(Value::as_str) == Some("assistant"))?;
    assistant
        .get("content")
        .and_then(|c| match c {
            Value::String(s) => Some(s.clone()),
            Value::Array(parts) => {
                let text = parts
                    .iter()
                    .filter_map(|p| {
                        p.get("text")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>()
                    .join("");
                Some(text)
            }
            _ => None,
        })
        .map(|s| s.trim().to_string())
}

/// The executor injected into the KnowledgeWorker. It has no fixed binding: the
/// preset (and therefore the runtime) is re-read from「模型与密钥」on every
/// call, so a manual switch applies to the very next completion.
pub struct KnowledgePiExecutor {
    app: AppHandle,
    pi_version: String,
}

impl KnowledgePiExecutor {
    pub fn new(app: AppHandle, pi_version: String) -> Self {
        Self { app, pi_version }
    }

    fn resolve_identity(&self) -> ModelIdentity {
        let binding = SettingsStore::get(&self.app)
            .ok()
            .flatten()
            .as_ref()
            .and_then(|settings| resolve_selected_preset(settings).ok())
            .map(preset_binding)
            .unwrap_or_else(|| PresetBinding {
                preset_id: String::new(),
                model_id: String::new(),
                wire_api: String::new(),
            });
        ModelIdentity {
            preset_id: binding.preset_id.clone(),
            provider_catalog_id: binding.preset_id,
            model_id: binding.model_id,
            wire_api: binding.wire_api,
            pi_version: self.pi_version.clone(),
            profile_version: KNOWLEDGE_PROFILE_VERSION.to_string(),
        }
    }
}

impl KnowledgeModelExecutor for KnowledgePiExecutor {
    fn identity(&self) -> ModelIdentity {
        self.resolve_identity()
    }

    fn complete(
        &self,
        request: CompletionRequest,
        cancel: CancellationToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<String, KnowledgeError>> + Send + '_>,
    > {
        Box::pin(run_strict_completion(&self.app, request, cancel))
    }
}

/// Start the KnowledgeWorker after the engine server is up. Call once.
pub(crate) const fn knowledge_startup_order() -> [&'static str; 4] {
    ["deletion_recovery", "history_migration", "discovery", "worker_lease"]
}

/// The four independent cadence layers (plan §4.2). ① must lead: the ledger
/// rebuild produces the intervals every later layer consumes, and the legacy
/// narrative scheduler is retired in the same build (§4.3 — the rebuild tick
/// is wired BEFORE legacy is stopped).
pub(crate) const CADENCE_LAYERS: [&str; 4] = ["reconcile", "summarize", "extract", "distill"];

/// The settings-driven cadence (13 keys in `settings.extra`, defaults and
/// ranges validated in the engine). Re-read before every tick: a settings
/// change applies on the next cycle with no restart. Missing settings fall
/// back to the plan defaults.
fn cadence_config(app: &AppHandle) -> KnowledgeCadenceConfig {
    SettingsStore::get(app)
        .ok()
        .flatten()
        .map(|settings| KnowledgeCadenceConfig::from_extra(&settings.extra))
        .unwrap_or_default()
}

/// Small startup stagger so ②③④ never race ①'s first rebuild — the discovery
/// scans read the ledger the reconcile tick just produced.
const CADENCE_STARTUP_STAGGER: Duration = Duration::from_secs(30);

/// ① 活动间隔重建：独立节拍，`reconcile_range(now - window, now)`，不再依赖
/// legacy 叙事生成路径的「读台账顺带 refresh」副作用（plan §4.2.1）。重启后
/// 立即跑第一轮——legacy 停用的前提是本 tick 已有产出。窗口与合并/吸收阈值
/// 全部可配；每轮记录重建出的间隔数作为可观测产出。
fn spawn_reconcile_tick(app: AppHandle, db: Arc<screenpipe_db::DatabaseManager>) {
    tokio::spawn(async move {
        loop {
            let config = cadence_config(&app);
            let end = chrono::Utc::now();
            let start = end - chrono::Duration::hours(config.reconcile_window_hours as i64);
            let policy = SegmentationPolicy {
                merge_gap: chrono::Duration::minutes(config.activity_merge_gap_minutes as i64),
                min_dwell: chrono::Duration::seconds(config.activity_min_dwell_seconds as i64),
            };
            match activity_ledger::reconcile_range_with_policy(&db, start, end, policy).await {
                Ok(intervals) => {
                    tracing::info!(
                        intervals,
                        window_hours = config.reconcile_window_hours,
                        "knowledge: ① activity ledger rebuilt"
                    );
                }
                Err(error) => {
                    tracing::warn!(%error, "knowledge: ① activity ledger rebuild failed");
                }
            }
            tokio::time::sleep(Duration::from_secs(config.reconcile_minutes * 60)).await;
        }
    });
}

/// ② 间隔摘要扫描：独立节拍 + 可配回看窗口/批大小/短段阈值（plan §4.2）。
fn spawn_summarize_tick(app: AppHandle, db: Arc<screenpipe_db::DatabaseManager>) {
    tokio::spawn(async move {
        tokio::time::sleep(CADENCE_STARTUP_STAGGER).await;
        loop {
            let config = cadence_config(&app);
            let since =
                chrono::Utc::now() - chrono::Duration::hours(config.discovery_lookback_hours as i64);
            let limits = screenpipe_engine::knowledge::summarize::DiscoveryLimits {
                batch: config.discovery_batch as i64,
                min_dwell_seconds: config.activity_min_dwell_seconds as i64,
            };
            match screenpipe_engine::knowledge::summarize::discover_and_enqueue(&db, since, limits)
                .await
            {
                Ok(n) if n > 0 => tracing::info!("knowledge: enqueued {n} summarize jobs"),
                Ok(_) => {}
                Err(e) => tracing::warn!("knowledge: summarize discovery failed: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(config.summarize_minutes * 60)).await;
        }
    });
}

/// ③ WorkUnit 抽取扫描：独立节拍 + 可配回看窗口/批大小（plan §4.2）。
fn spawn_extract_tick(app: AppHandle, db: Arc<screenpipe_db::DatabaseManager>) {
    tokio::spawn(async move {
        tokio::time::sleep(CADENCE_STARTUP_STAGGER).await;
        loop {
            let config = cadence_config(&app);
            let since =
                chrono::Utc::now() - chrono::Duration::hours(config.discovery_lookback_hours as i64);
            let limits = screenpipe_engine::knowledge::summarize::DiscoveryLimits {
                batch: config.discovery_batch as i64,
                min_dwell_seconds: config.activity_min_dwell_seconds as i64,
            };
            match screenpipe_engine::knowledge::extract::discover_and_enqueue(&db, since, limits)
                .await
            {
                Ok(n) if n > 0 => tracing::info!("knowledge: enqueued {n} extraction jobs"),
                Ok(_) => {}
                Err(e) => tracing::warn!("knowledge: extraction discovery failed: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(config.work_unit_minutes * 60)).await;
        }
    });
}

/// ④ 知识蒸馏：粗粒度定时 + 三机制投递（模型提名/变更驱动/冷却期，
/// plan §4.2.2）。定时只决定「什么时候看一眼」；编译任务由引擎侧 compile
/// 完成后回写蒸馏状态，失败任务保持终态、等待用户手动重试。
fn spawn_distill_tick(app: AppHandle, db: Arc<screenpipe_db::DatabaseManager>) {
    tokio::spawn(async move {
        tokio::time::sleep(CADENCE_STARTUP_STAGGER).await;
        loop {
            let config = cadence_config(&app);
            match distill::discover_and_enqueue(&db, config.distill_cooldown_days).await {
                Ok(n) if n > 0 => tracing::info!("knowledge: distill enqueued {n} compile jobs"),
                Ok(_) => {}
                Err(e) => tracing::warn!("knowledge: distill discovery failed: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(config.distill_hours * 3_600)).await;
        }
    });
}

pub async fn start_knowledge_worker(app: &AppHandle) {
    {
        let state = app.state::<KnowledgeRuntimeState>();
        if state.worker_handle().await.is_some() {
            return;
        }
    }
    // The engine server constructs the shared state asynchronously; wait for
    // it instead of giving up (a missing worker silently idles every queue).
    let shared = {
        let mut found: Option<Arc<screenpipe_engine::knowledge::KnowledgeShared>> = None;
        for _ in 0..60 {
            if let Some(shared) = screenpipe_engine::knowledge::shared() {
                found = Some(shared.clone());
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        match found {
            Some(shared) => shared,
            None => {
                tracing::warn!("knowledge: engine shared state never appeared; worker not started");
                return;
            }
        }
    };
    // Journal recovery is a startup gate: no discovery or worker lease may be
    // taken while an interrupted deletion can still leave old bodies visible.
    if let Err(error) = shared.ensure_recovered().await {
        tracing::warn!("knowledge: deletion recovery failed; worker not started: {error}");
        return;
    }
    if let Err(error) = activate_public_knowledge_owner(&shared.db).await {
        tracing::warn!("knowledge: public task owner activation failed; worker not started: {error}");
        return;
    }
    // History preservation migration first (pauses the old writer briefly).
    crate::knowledge_migration::migrate_activity_history(app).await;
    shared.refresh_barrier().await;

    // Four independent cadence layers (plan §4.2): ① ledger rebuild, ②
    // interval summaries, ③ WorkUnit extraction, ④ distillation. Each owns
    // its tick, discovery window and budgets from settings; ① runs first at
    // startup so the legacy narrative scheduler can stay retired (§4.3).
    // Office auto-sync (five minutes) is separate (office_runtime).
    spawn_reconcile_tick(app.clone(), shared.db.clone());
    spawn_summarize_tick(app.clone(), shared.db.clone());
    spawn_extract_tick(app.clone(), shared.db.clone());
    spawn_distill_tick(app.clone(), shared.db.clone());
    let pi_version = detect_pi_version().await;
    let executor = Arc::new(KnowledgePiExecutor::new(app.clone(), pi_version));
    let mut handlers = crate::office_runtime::build_handlers_with_office(app).await;
    handlers.register(
        screenpipe_db::KnowledgeJobKind::Summarize,
        screenpipe_engine::knowledge::summarize::summarize_handler(),
    );
    handlers.register(
        screenpipe_db::KnowledgeJobKind::Extract,
        screenpipe_engine::knowledge::extract::extract_handler(),
    );
    handlers.register(
        screenpipe_db::KnowledgeJobKind::BackfillExtract,
        screenpipe_engine::knowledge::extract::backfill_extract_handler(),
    );
    handlers.register(
        screenpipe_db::KnowledgeJobKind::Compile,
        screenpipe_engine::knowledge::compile::compile_handler(),
    );
    let handle = worker::start_public(
        shared.db.clone(),
        executor.clone(),
        handlers,
        WorkerConfig {
            worker_id: format!("desktop-{}", uuid::Uuid::new_v4()),
            lease_ms: 30_000,
            heartbeat_ms: 5_000,
            idle_sleep_ms: 1_500,
            limits: cadence_config(app).job_limits(),
        },
    );
    shared.set_executor(executor.clone()).await;
    shared.set_worker(handle.clone()).await;
    {
        let state = app.state::<KnowledgeRuntimeState>();
        *state.worker.write().await = Some(handle.clone());
    }
    let db = shared.db.clone();
    let _ = db.knowledge_reap_expired_leases().await;
    crate::office_runtime::start_auto_sync(app);
    tracing::info!("knowledge worker started");
}

#[cfg(test)]
mod knowledge_runtime_tests {
    use super::{knowledge_startup_order, KnowledgeCadenceConfig, CADENCE_LAYERS};

    #[test]
    fn knowledge_runtime_recovery_precedes_discovery_and_worker_lease() {
        let order = knowledge_startup_order();
        assert_eq!(order[0], "deletion_recovery");
        assert!(order.iter().position(|step| *step == "discovery")
            < order.iter().position(|step| *step == "worker_lease"));
    }

    /// §4.2/§4.3 顺序硬约束：① 重建 tick 先于其余层；四层各自独立成节拍。
    #[test]
    fn reconcile_tick_leads_and_layers_are_independent() {
        assert_eq!(CADENCE_LAYERS[0], "reconcile", "① 必须最先");
        assert_eq!(CADENCE_LAYERS.len(), 4);
        for (i, layer) in CADENCE_LAYERS.iter().enumerate() {
            assert!(CADENCE_LAYERS.iter().position(|l| l == layer) == Some(i));
        }
    }

    /// 设置缺席时四层都以计划默认值运行（用户不改也能跑）。
    #[test]
    fn cadence_defaults_apply_without_settings() {
        let config = KnowledgeCadenceConfig::default();
        assert_eq!(config.reconcile_minutes, 5);
        assert_eq!(config.reconcile_window_hours, 2);
        assert_eq!(config.summarize_minutes, 15);
        assert_eq!(config.work_unit_minutes, 60);
        assert_eq!(config.distill_hours, 24);
        assert_eq!(config.distill_cooldown_days, 7);
        assert_eq!(config.discovery_lookback_hours, 26);
        assert_eq!(config.discovery_batch, 20);
    }
}

/// Stop the worker on app exit; in-flight steps are cancelled and uncommitted.
pub async fn stop_knowledge_worker(app: &AppHandle) {
    let state = app.state::<KnowledgeRuntimeState>();
    if let Some(handle) = state.worker_handle().await {
        handle.cancel();
    }
    // Give the loop a moment to observe cancellation.
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// Best-effort Pi version probe for the binding record (identity only).
async fn detect_pi_version() -> String {
    let probe = tokio::process::Command::new("pi")
        .arg("--version")
        .output();
    match tokio::time::timeout(Duration::from_secs(5), probe).await {
        Ok(Ok(out)) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.split_whitespace().last().unwrap_or("unknown").to_string()
        }
        _ => "unknown".to_string(),
    }
}

async fn activate_public_knowledge_owner(
    db: &Arc<screenpipe_db::DatabaseManager>,
) -> Result<(), String> {
    let current = db
        .task_get_owner_state("knowledge")
        .await
        .map_err(|error| error.to_string())?;
    let (generation, expected_generation) = match current {
        Some(state) if state.migration_state == "unified" => {
            return Ok(())
        }
        Some(state) => (state.owner_generation + 1, Some(state.owner_generation)),
        None => (1, None),
    };
    let changed = db
        .task_set_owner_state(
            &screenpipe_db::TaskOwnerState {
                kind: "knowledge".to_string(),
                owner_generation: generation,
                migration_state: "unified".to_string(),
                checkpoint: Some("desktop_public_task_worker".to_string()),
            },
            expected_generation,
        )
        .await
        .map_err(|error| error.to_string())?;
    if !changed {
        return Err("knowledge owner generation changed during activation".to_string());
    }
    db.task_activate_owner_generation("knowledge", generation)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}
