// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Shared, storage-independent contracts for the local task runtime.
//!
//! The task service owns lifecycle identity and durable progress. Business
//! handlers remain in their owning crates; these types contain references and
//! safe metadata, never source or model secret bodies.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TASK_EVENT_MAX_BYTES: usize = 64 * 1024;
pub const TASK_MESSAGE_RETENTION_LIMIT: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    HistoryMigration,
    ActivitySummary,
    PipeRun,
}

impl TaskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HistoryMigration => "history_migration",
            Self::ActivitySummary => "activity_summary",
            Self::PipeRun => "pipe_run",
        }
    }
}

impl std::str::FromStr for TaskKind {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "history_migration" => Self::HistoryMigration,
            "activity_summary" => Self::ActivitySummary,
            "pipe_run" => Self::PipeRun,
            _ => return Err(()),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskOrigin {
    Builtin,
    User,
    Connection,
}

impl TaskOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::User => "user",
            Self::Connection => "connection",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskTrigger {
    pub kind: String,
    pub expression: Option<String>,
}

impl Default for TaskTrigger {
    fn default() -> Self {
        Self {
            kind: "manual".to_string(),
            expression: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskResourceClass {
    Interactive,
    Extract,
    Backfill,
    OfficeIo,
    UserPipe,
}

impl TaskResourceClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Extract => "extract",
            Self::Backfill => "backfill",
            Self::OfficeIo => "office_io",
            Self::UserPipe => "user_pipe",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRetryPolicy {
    pub max_attempts: u32,
    pub network_retries: u32,
    pub invalid_output_retries: u32,
    pub backoff_ms: u64,
}

impl Default for TaskRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            network_retries: 1,
            invalid_output_retries: 1,
            backoff_ms: 1_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskDefinition {
    pub definition_id: String,
    pub kind: TaskKind,
    pub origin: TaskOrigin,
    pub schema_version: u32,
    pub config_revision: String,
    pub config_ref: Option<String>,
    pub trigger: TaskTrigger,
    pub enabled: bool,
    pub resource_class: TaskResourceClass,
    pub retry_policy: TaskRetryPolicy,
    pub model_binding_policy: Option<String>,
    pub revision: i64,
    pub owner_generation: i64,
    pub migration_state: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Running,
    Paused,
    Cancelling,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    NeedsAttention,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Cancelling => "cancelling",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
            Self::NeedsAttention => "needs_attention",
        }
    }
}

impl std::str::FromStr for TaskState {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "paused" => Self::Paused,
            "cancelling" => Self::Cancelling,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "timed_out" => Self::TimedOut,
            "cancelled" => Self::Cancelled,
            "needs_attention" => Self::NeedsAttention,
            _ => return Err(()),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskRun {
    pub run_id: String,
    pub definition_id: Option<String>,
    pub definition_revision: String,
    pub root_run_id: String,
    pub parent_run_id: Option<String>,
    pub retry_of: Option<String>,
    pub trigger_key: String,
    pub input_hash: String,
    pub input_refs: Value,
    pub config_snapshot: Value,
    pub state: TaskState,
    pub revision: i64,
    pub priority: i32,
    pub not_before: Option<String>,
    pub deadline: Option<String>,
    pub cursor: Option<String>,
    pub lease_owner: Option<String>,
    pub lease_token: Option<String>,
    pub lease_expires_at: Option<String>,
    pub owner_generation: i64,
    pub output_refs: Value,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskAttempt {
    pub attempt_id: String,
    pub run_id: String,
    pub attempt_no: u32,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub runtime_binding: Option<Value>,
    pub model_calls_used: u32,
    pub retry_reason: Option<String>,
    pub outcome: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskEvent {
    pub run_id: String,
    pub seq: i64,
    pub attempt_id: Option<String>,
    pub phase: String,
    pub event_type: String,
    pub timestamp: String,
    pub safe_metadata: Value,
    pub payload_ref: Option<String>,
    pub output_refs: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskControl {
    Pause,
    Resume,
    Cancel,
    Retry,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskLegacyMapping {
    pub legacy_namespace: String,
    pub legacy_id: String,
    pub definition_id: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskOwnerState {
    pub kind: String,
    pub owner_generation: i64,
    pub migration_state: String,
    pub checkpoint: Option<String>,
}

pub fn new_revision() -> String {
    uuid::Uuid::new_v4().to_string()
}
