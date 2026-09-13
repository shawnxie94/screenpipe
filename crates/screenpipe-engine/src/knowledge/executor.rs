// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Strict model execution boundary. The engine never resolves credentials
//! and never picks models: the desktop shell injects one
//! [`KnowledgeModelExecutor`] bound to the user-specified preset/model, and the
//! worker counts every call against the per-job budget (max 3).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tokio::sync::Semaphore;

use super::types::KnowledgeError;

/// One ordinary completion request. No tools, no agent loop, no streaming.
#[derive(Clone, Debug)]
pub struct CompletionRequest {
    /// Chinese system prompt from the task profile (`knowledge-extract-v1`).
    pub system: String,
    pub user_prompt: String,
    pub max_output_tokens: u32,
    /// Hard wall-clock budget for this single call (≤45s background).
    pub timeout: Duration,
    /// `extract` | `compile` | `answer` — for logs only, never prompts.
    pub purpose: &'static str,
}

/// Identity of the bound model, recorded with executions (no endpoints, no
/// keys — the endpoint fingerprint stays on the desktop side). `preset_id`
/// empty means nothing is selected yet.
#[derive(Clone, Debug)]
pub struct ModelIdentity {
    pub preset_id: String,
    pub provider_catalog_id: String,
    pub model_id: String,
    pub wire_api: String,
    pub pi_version: String,
    pub profile_version: String,
}

pub trait KnowledgeModelExecutor: Send + Sync {
    /// Resolved live on every call: the knowledge follows the preset the user
    /// selects in「模型与密钥」, so manual switches take effect immediately.
    fn identity(&self) -> ModelIdentity;

    /// One ordinary completion. Implementations must not fall back to a
    /// different model, retry internally, or leak secrets into errors.
    fn complete(
        &self,
        request: CompletionRequest,
        cancel: CancellationToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<String, KnowledgeError>> + Send + '_>,
    >;
}

/// Conservative resource defaults (TRD §7.1). The settings-configurable
/// equivalents live in [`KnowledgeJobLimits`]; these constants remain the
/// fallback for callers that have no settings at hand (interactive answers,
/// isolated tests).
pub struct KnowledgeLimits;

impl KnowledgeLimits {
    /// Per single model call in the background.
    pub const BACKGROUND_CALL_TIMEOUT: Duration = Duration::from_secs(45);
    /// Total wall clock for one background step.
    pub const STEP_TIMEOUT: Duration = Duration::from_secs(120);
    /// Model calls allowed per job (network retry 1 + repair 1 included).
    pub const MAX_MODEL_CALLS: u32 = 3;
    /// Answer request: max time spent waiting in the queue.
    pub const ANSWER_QUEUE_DEADLINE: Duration = Duration::from_secs(10);
    /// Answer request: total deadline including retrieval and completion.
    pub const ANSWER_TOTAL_DEADLINE: Duration = Duration::from_secs(60);
    /// Max output tokens for one completion.
    pub const MAX_OUTPUT_TOKENS: u32 = 4_096;
}

/// Per-job execution budgets (plan §4.2): single-call timeout, total step
/// timeout and the model-call cap — all fed from the settings keys
/// `knowledgeCallTimeoutSeconds` / `knowledgeStepTimeoutSeconds` /
/// `knowledgeMaxModelCallsPerJob` via the desktop shell.
#[derive(Clone, Copy, Debug)]
pub struct KnowledgeJobLimits {
    pub call_timeout: Duration,
    pub step_timeout: Duration,
    pub max_model_calls: u32,
}

impl Default for KnowledgeJobLimits {
    fn default() -> Self {
        Self {
            call_timeout: KnowledgeLimits::BACKGROUND_CALL_TIMEOUT,
            step_timeout: KnowledgeLimits::STEP_TIMEOUT,
            max_model_calls: KnowledgeLimits::MAX_MODEL_CALLS,
        }
    }
}

/// One process-wide model permit shared by interactive answers and background
/// handlers. A queued background task cannot consume the permit forever and a
/// chat request never deadlocks behind a nested child completion.
static MODEL_LANE: Semaphore = Semaphore::const_new(1);

/// Wrapper enforcing the per-job call budget. The worker creates one per
/// claimed job; the handler sees `Refused` once the budget is spent and must
/// fail the job instead of looping.
#[derive(Clone)]
pub struct BudgetedExecutor {
    inner: Arc<dyn KnowledgeModelExecutor>,
    calls: Arc<AtomicU32>,
    max_calls: u32,
    /// Optional cap on the per-call timeout: `complete` tightens the
    /// request's own timeout to this value so the configured
    /// `knowledgeCallTimeoutSeconds` binds regardless of what the handler
    /// asked for. The enforcement itself stays with the inner executor, which
    /// owns the process cleanup on expiry.
    call_timeout: Option<Duration>,
    persisted_job: Option<(Arc<screenpipe_db::DatabaseManager>, i64, String)>,
}

impl BudgetedExecutor {
    pub fn new(inner: Arc<dyn KnowledgeModelExecutor>, max_calls: u32) -> Self {
        Self {
            inner,
            calls: Arc::new(AtomicU32::new(0)),
            max_calls: max_calls.max(1),
            call_timeout: None,
            persisted_job: None,
        }
    }

    pub fn new_for_job(
        inner: Arc<dyn KnowledgeModelExecutor>,
        limits: KnowledgeJobLimits,
        db: Arc<screenpipe_db::DatabaseManager>,
        job_id: i64,
        lease_token: String,
    ) -> Self {
        let mut executor = Self::new(inner, limits.max_model_calls);
        executor.call_timeout = Some(limits.call_timeout);
        executor.persisted_job = Some((db, job_id, lease_token));
        executor
    }

    pub fn used_calls(&self) -> u32 {
        self.calls.load(Ordering::Relaxed)
    }

    /// Live model binding for input hashing (no secrets). The inner executor
    /// re-resolves it on every call, so a preset switch changes the hash
    /// family and summaries recompute instead of being swallowed by the old
    /// idempotency.
    pub fn identity(&self) -> ModelIdentity {
        self.inner.identity()
    }

    pub async fn complete(
        &self,
        mut request: CompletionRequest,
        cancel: CancellationToken,
    ) -> Result<String, KnowledgeError> {
        // The configured per-call cap always wins over the handler's request.
        if let Some(cap) = self.call_timeout {
            request.timeout = request.timeout.min(cap);
        }
        let prior = self.calls.fetch_add(1, Ordering::Relaxed);
        if prior >= self.max_calls {
            return Err(KnowledgeError::new(
                "model_budget_exhausted",
                "单任务模型调用次数已达上限",
                false,
            ));
        }
        if let Some((db, job_id, lease_token)) = &self.persisted_job {
            let counted = db
                .knowledge_job_model_call(*job_id, lease_token)
                .await
                .map_err(|e| KnowledgeError::new("lease_lost", e.to_string(), true))?;
            if !counted {
                return Err(KnowledgeError::new(
                    "lease_lost",
                    "任务租约已失效，模型结果不会提交",
                    true,
                ));
            }
        }
        let permit = match tokio::time::timeout(Duration::from_secs(10), MODEL_LANE.acquire()).await {
            Ok(Ok(permit)) => permit,
            _ => {
                return Err(KnowledgeError::new(
                    "resource_busy",
                    "模型资源当前被其他任务占用",
                    true,
                ));
            }
        };
        let result = self.inner.complete(request, cancel).await;
        drop(permit);
        result
    }
}
