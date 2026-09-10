// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Public projection and engine facade for the shared durable task store.

use std::{collections::HashSet, fmt, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use screenpipe_db::{
    KnowledgeJobKind, DatabaseManager, TaskControl, TaskDefinition, TaskKind, TaskOrigin,
    TaskResourceClass, TaskRetryPolicy, TaskRunRequest, TaskState, TaskTrigger,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::server::AppState;

#[derive(Clone)]
pub struct TaskService {
    db: Arc<DatabaseManager>,
}

/// A user Pipe is authored by its `pipe.md`. The public task catalog mirrors
/// that file as a read-only definition so automation can show one inventory
/// without creating a second configuration source.
#[derive(Clone, Debug)]
pub struct UserPipeTaskSpec {
    pub name: String,
    pub config_revision: String,
    pub enabled: bool,
    pub trigger: TaskTrigger,
    pub model_binding_policy: Option<String>,
}

impl TaskService {
    pub fn new(db: Arc<DatabaseManager>) -> Self {
        Self { db }
    }

    pub fn db(&self) -> &Arc<DatabaseManager> {
        &self.db
    }

    pub async fn register(&self, definition: &TaskDefinition) -> Result<bool, sqlx::Error> {
        self.db.task_upsert_definition(definition, None).await
    }

    pub async fn register_builtin(
        &self,
        definition_id: &str,
        kind: TaskKind,
        resource_class: TaskResourceClass,
    ) -> Result<bool, sqlx::Error> {
        self.register_catalog_definition(
            definition_id,
            kind,
            TaskOrigin::Builtin,
            resource_class,
            "builtin-v1",
        )
        .await
    }

    pub async fn register_connection(
        &self,
        definition_id: &str,
        kind: TaskKind,
        resource_class: TaskResourceClass,
    ) -> Result<bool, sqlx::Error> {
        self.register_catalog_definition(
            definition_id,
            kind,
            TaskOrigin::Connection,
            resource_class,
            "connection-v1",
        )
        .await
    }

    async fn register_catalog_definition(
        &self,
        definition_id: &str,
        kind: TaskKind,
        origin: TaskOrigin,
        resource_class: TaskResourceClass,
        config_revision: &str,
    ) -> Result<bool, sqlx::Error> {
        self.db
            .task_upsert_definition(
                &TaskDefinition {
                    definition_id: definition_id.to_string(),
                    kind,
                    origin,
                    schema_version: 1,
                    config_revision: config_revision.to_string(),
                    config_ref: None,
                    trigger: TaskTrigger::default(),
                    enabled: true,
                    resource_class,
                    retry_policy: TaskRetryPolicy::default(),
                    model_binding_policy: Some("follow_current".to_string()),
                    revision: 0,
                    owner_generation: 0,
                    migration_state: "unified".to_string(),
                },
                None,
            )
            .await
    }

    pub async fn register_default_definitions(&self) -> Result<(), sqlx::Error> {
        for (id, kind, resource) in [
            (
                "brain.extract",
                TaskKind::KnowledgeExtract,
                TaskResourceClass::Extract,
            ),
            (
                "brain.compile",
                TaskKind::KnowledgeCompile,
                TaskResourceClass::Extract,
            ),
            (
                "brain.backfill",
                TaskKind::KnowledgeBackfill,
                TaskResourceClass::Backfill,
            ),
            (
                "activity.summary",
                TaskKind::ActivitySummary,
                TaskResourceClass::Extract,
            ),
            ("pipe.run", TaskKind::PipeRun, TaskResourceClass::UserPipe),
        ] {
            self.register_builtin(id, kind, resource).await?;
        }
        self.register_connection(
            "office.sync",
            TaskKind::OfficeSync,
            TaskResourceClass::OfficeIo,
        )
        .await?;
        Ok(())
    }

    pub async fn sync_user_pipe_definitions(
        &self,
        pipes: &[UserPipeTaskSpec],
    ) -> Result<(), sqlx::Error> {
        let seen = pipes
            .iter()
            .map(|pipe| user_pipe_definition_id(&pipe.name))
            .collect::<HashSet<_>>();
        for pipe in pipes {
            let next = TaskDefinition {
                definition_id: user_pipe_definition_id(&pipe.name),
                kind: TaskKind::PipeRun,
                origin: TaskOrigin::User,
                schema_version: 1,
                config_revision: pipe.config_revision.clone(),
                config_ref: Some(format!("pipe:{}", pipe.name)),
                trigger: pipe.trigger.clone(),
                enabled: pipe.enabled,
                resource_class: TaskResourceClass::UserPipe,
                retry_policy: TaskRetryPolicy::default(),
                model_binding_policy: pipe.model_binding_policy.clone(),
                revision: 0,
                owner_generation: 0,
                migration_state: "unified_projection".to_string(),
            };
            let current = self.db.task_get_definition(&next.definition_id).await?;
            let unchanged = current.as_ref().is_some_and(|current| {
                current.kind == next.kind
                    && current.origin == next.origin
                    && current.config_revision == next.config_revision
                    && current.config_ref == next.config_ref
                    && current.trigger == next.trigger
                    && current.enabled == next.enabled
                    && current.model_binding_policy == next.model_binding_policy
            });
            if unchanged {
                continue;
            }
            let expected_revision = current.as_ref().map(|current| current.revision);
            let _ = self
                .db
                .task_upsert_definition(&next, expected_revision)
                .await?;
        }
        for current in self.db.task_list_definitions().await? {
            if current.origin != TaskOrigin::User
                || current.kind != TaskKind::PipeRun
                || current.definition_id == "pipe.run"
                || seen.contains(&current.definition_id)
                || current.migration_state == "retired"
            {
                continue;
            }
            let mut retired = current.clone();
            retired.enabled = false;
            retired.migration_state = "retired".to_string();
            let _ = self
                .db
                .task_upsert_definition(&retired, Some(current.revision))
                .await?;
        }
        Ok(())
    }

    pub async fn definitions(&self) -> Result<Vec<TaskDefinition>, sqlx::Error> {
        self.db.task_list_definitions().await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn trigger(
        &self,
        definition_id: &str,
        trigger: TaskTrigger,
        trigger_key: &str,
        input_hash: &str,
        input_refs: Value,
        deadline: Option<DateTime<Utc>>,
        owner_generation: i64,
    ) -> Result<(String, bool), TaskServiceError> {
        let definition = self
            .db
            .task_get_definition(definition_id)
            .await
            .map_err(TaskServiceError::Database)?
            .ok_or(TaskServiceError::DefinitionNotFound)?;
        if !definition.enabled {
            return Err(TaskServiceError::DefinitionDisabled);
        }
        if definition.trigger != trigger {
            return Err(TaskServiceError::TriggerMismatch);
        }
        self.db
            .task_start_run(&TaskRunRequest {
                definition_id: Some(definition_id.to_string()),
                definition_revision: definition.config_revision,
                root_run_id: None,
                parent_run_id: None,
                retry_of: None,
                trigger_key: trigger_key.to_string(),
                input_hash: input_hash.to_string(),
                input_refs,
                config_snapshot: json!({}),
                priority: task_priority(definition.kind),
                not_before: None,
                deadline: deadline.map(|value| value.to_rfc3339()),
                owner_generation,
            })
            .await
            .map_err(TaskServiceError::Database)
    }
}

pub async fn register_default_definitions(db: Arc<DatabaseManager>) -> Result<(), sqlx::Error> {
    TaskService::new(db).register_default_definitions().await
}

#[derive(Debug)]
pub enum TaskServiceError {
    DefinitionNotFound,
    DefinitionDisabled,
    TriggerMismatch,
    Database(sqlx::Error),
}

impl fmt::Display for TaskServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DefinitionNotFound => f.write_str("task definition not found"),
            Self::DefinitionDisabled => f.write_str("task definition is disabled"),
            Self::TriggerMismatch => f.write_str("task trigger does not match definition"),
            Self::Database(error) => write!(f, "database error: {error}"),
        }
    }
}

impl std::error::Error for TaskServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::DefinitionNotFound | Self::DefinitionDisabled | Self::TriggerMismatch => None,
        }
    }
}

fn task_priority(kind: TaskKind) -> i32 {
    match kind {
        TaskKind::ActivitySummary | TaskKind::HistoryMigration => 5,
        TaskKind::KnowledgeExtract => 10,
        TaskKind::KnowledgeCompile | TaskKind::KnowledgeBackfill => 20,
        TaskKind::PipeRun => 15,
        TaskKind::OfficeSync => 30,
    }
}

pub fn user_pipe_definition_id(name: &str) -> String {
    format!("pipe.user.{}", screenpipe_db::fingerprint(&[name]))
}

fn knowledge_kind_for_task(kind: TaskKind) -> Option<KnowledgeJobKind> {
    Some(match kind {
        TaskKind::KnowledgeExtract => KnowledgeJobKind::Extract,
        TaskKind::KnowledgeCompile => KnowledgeJobKind::Compile,
        TaskKind::KnowledgeBackfill => KnowledgeJobKind::BackfillExtract,
        TaskKind::OfficeSync => KnowledgeJobKind::OfficeSync,
        TaskKind::HistoryMigration => KnowledgeJobKind::HistoryMigration,
        TaskKind::ActivitySummary | TaskKind::PipeRun => return None,
    })
}

#[derive(Debug, Deserialize)]
struct RunRequest {
    #[serde(default = "default_trigger")]
    trigger_key: String,
    #[serde(default)]
    input_hash: String,
    #[serde(default)]
    input_refs: Value,
    #[serde(default)]
    config_snapshot: Value,
    #[serde(default)]
    priority: Option<i32>,
    #[serde(default)]
    deadline: Option<String>,
}

fn default_trigger() -> String {
    "manual".to_string()
}

#[derive(Debug, Deserialize)]
struct ControlRequest {
    control: TaskControl,
    expected_revision: i64,
}

#[derive(Debug, Deserialize)]
struct EventQuery {
    #[serde(default)]
    after_seq: i64,
    #[serde(default = "default_event_limit")]
    limit: u32,
}

#[derive(Debug, Deserialize)]
struct RunListQuery {
    #[serde(default = "default_run_limit")]
    limit: u32,
}

fn default_run_limit() -> u32 {
    100
}

fn default_event_limit() -> u32 {
    100
}

fn error(status: StatusCode, code: &str, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({
            "code": code,
            "message": message.into(),
            "retryable": status == StatusCode::SERVICE_UNAVAILABLE,
        })),
    )
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/definitions",
            get(list_definitions).post(create_definition),
        )
        .route("/definitions/:definition_id", patch(update_definition))
        .route("/definitions/:definition_id/runs", post(start_run))
        .route("/runs", get(list_runs))
        .route("/runs/:run_id", get(get_run))
        .route("/runs/:run_id/events", get(list_events))
        .route("/runs/:run_id/control", post(control_run))
}

async fn list_definitions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if let Some(pipe_manager) = &state.pipe_manager {
        let manager = pipe_manager.lock().await;
        if let Err(error) = manager.reload_pipes().await {
            tracing::warn!(%error, "task catalog: failed to reload user Pipes");
        }
        let specs = manager
            .list_pipes()
            .await
            .into_iter()
            .map(|status| UserPipeTaskSpec {
                name: status.config.name,
                config_revision: screenpipe_db::fingerprint(&[status.raw_content.as_str()]),
                enabled: status.config.enabled,
                trigger: if status.config.trigger.is_some() {
                    TaskTrigger {
                        kind: "event".to_string(),
                        expression: status
                            .config
                            .trigger
                            .as_ref()
                            .and_then(|trigger| serde_json::to_string(trigger).ok()),
                    }
                } else if status.config.schedule == "manual" {
                    TaskTrigger::default()
                } else {
                    TaskTrigger {
                        kind: "interval".to_string(),
                        expression: Some(status.config.schedule),
                    }
                },
                model_binding_policy: status
                    .config
                    .preset
                    .first()
                    .cloned()
                    .or_else(|| Some(status.config.model)),
            })
            .collect::<Vec<_>>();
        drop(manager);
        TaskService::new(state.db.clone())
            .sync_user_pipe_definitions(&specs)
            .await
            .map_err(|storage_error| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "task_storage",
                    storage_error.to_string(),
                )
            })?;
    }
    state
        .db
        .task_list_definitions()
        .await
        .map(|items| {
            let items = items
                .into_iter()
                .filter(|definition| {
                    definition.definition_id != "pipe.run"
                        && definition.migration_state != "retired"
                });
            Json(json!({
                "definitions": items.map(|definition| definition_json(&definition)).collect::<Vec<_>>(),
            }))
        })
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })
}

fn definition_json(definition: &TaskDefinition) -> Value {
    let mut value = serde_json::to_value(definition).unwrap_or_else(|_| json!({}));
    if let Value::Object(fields) = &mut value {
        let managed_by_connection = matches!(definition.origin, TaskOrigin::Connection);
        fields.insert(
            "managed_by_connection".to_string(),
            json!(managed_by_connection),
        );
        if managed_by_connection {
            fields.insert("configuration_target".to_string(), json!("connections"));
        }
        if definition.origin == TaskOrigin::User && definition.kind == TaskKind::PipeRun {
            fields.insert("managed_by_pipe".to_string(), json!(true));
            fields.insert("configuration_target".to_string(), json!("pipes"));
        }
    }
    value
}

#[derive(Debug, Deserialize)]
struct CreateDefinitionRequest {
    definition_id: String,
    kind: TaskKind,
    #[serde(default)]
    origin: Option<TaskOrigin>,
    config_revision: String,
    #[serde(default)]
    config_ref: Option<String>,
    #[serde(default)]
    trigger: TaskTrigger,
    #[serde(default = "default_enabled")]
    enabled: bool,
    resource_class: TaskResourceClass,
    #[serde(default)]
    retry_policy: TaskRetryPolicy,
    #[serde(default)]
    model_binding_policy: Option<String>,
    #[serde(default)]
    expected_revision: Option<i64>,
}

fn default_enabled() -> bool {
    true
}

async fn create_definition(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateDefinitionRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if matches!(
        request.origin.as_ref(),
        Some(TaskOrigin::Builtin | TaskOrigin::Connection)
    ) || is_managed_definition_id(&request.definition_id)
    {
        return Err(error(
            StatusCode::FORBIDDEN,
            "managed_definition_immutable",
            "内置任务和连接任务只能由本地运行时或连接配置注册",
        ));
    }
    let definition = TaskDefinition {
        definition_id: request.definition_id,
        kind: request.kind,
        origin: request.origin.unwrap_or(TaskOrigin::User),
        schema_version: 1,
        config_revision: request.config_revision,
        config_ref: request.config_ref,
        trigger: request.trigger,
        enabled: request.enabled,
        resource_class: request.resource_class,
        retry_policy: request.retry_policy,
        model_binding_policy: request.model_binding_policy,
        revision: 0,
        owner_generation: 0,
        migration_state: "unified".to_string(),
    };
    let id = definition.definition_id.clone();
    let changed = state
        .db
        .task_upsert_definition(&definition, request.expected_revision)
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?;
    if !changed {
        return Err(error(
            StatusCode::CONFLICT,
            "revision_conflict",
            "definition revision changed",
        ));
    }
    let definition = state
        .db
        .task_get_definition(&id)
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?
        .ok_or_else(|| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                "definition disappeared",
            )
        })?;
    Ok(Json(json!({ "definition": definition_json(&definition) })))
}

#[derive(Debug, Deserialize)]
struct UpdateDefinitionRequest {
    expected_revision: i64,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    config_revision: Option<String>,
    #[serde(default)]
    config_ref: Option<String>,
    #[serde(default)]
    trigger: Option<TaskTrigger>,
    #[serde(default)]
    model_binding_policy: Option<String>,
}

async fn update_definition(
    State(state): State<Arc<AppState>>,
    Path(definition_id): Path<String>,
    Json(request): Json<UpdateDefinitionRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut definition = state
        .db
        .task_get_definition(&definition_id)
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?
        .ok_or_else(|| {
            error(
                StatusCode::NOT_FOUND,
                "definition_not_found",
                "task definition not found",
            )
        })?;
    let is_connection = matches!(definition.origin, TaskOrigin::Connection);
    let is_user_pipe =
        definition.origin == TaskOrigin::User && definition.kind == TaskKind::PipeRun;
    let has_managed_fields = request.config_revision.is_some()
        || request.config_ref.is_some()
        || request.trigger.is_some()
        || request.model_binding_policy.is_some();
    if is_connection && (has_managed_fields || request.enabled.is_some()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "managed_definition_immutable",
            "连接任务的账号、范围、自动同步和触发配置必须在连接页面维护",
        ));
    }
    if is_user_pipe && (has_managed_fields || request.enabled.is_some()) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "pipe_definition_immutable",
            "用户任务的 pipe.md、启停和触发配置必须在“我的任务”编辑器维护",
        ));
    }
    if request.enabled.is_none() && !has_managed_fields {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "empty_update",
            "至少需要提供一个可更新字段",
        ));
    }
    if let Some(enabled) = request.enabled {
        definition.enabled = enabled;
    }
    if let Some(config_revision) = request.config_revision {
        definition.config_revision = config_revision;
    }
    if let Some(config_ref) = request.config_ref {
        definition.config_ref = Some(config_ref);
    }
    if let Some(trigger) = request.trigger {
        definition.trigger = trigger;
    }
    if let Some(model_binding_policy) = request.model_binding_policy {
        definition.model_binding_policy = Some(model_binding_policy);
    }
    let changed = state
        .db
        .task_upsert_definition(&definition, Some(request.expected_revision))
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?;
    if !changed {
        return Err(error(
            StatusCode::CONFLICT,
            "revision_conflict",
            "definition revision changed",
        ));
    }
    let definition = state
        .db
        .task_get_definition(&definition_id)
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?
        .ok_or_else(|| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                "definition disappeared",
            )
        })?;
    Ok(Json(json!({ "definition": definition_json(&definition) })))
}

fn is_managed_definition_id(definition_id: &str) -> bool {
    matches!(
        definition_id,
        "brain.extract"
            | "brain.compile"
            | "brain.backfill"
            | "activity.summary"
            | "office.sync"
            | "pipe.run"
    )
}

async fn start_run(
    State(state): State<Arc<AppState>>,
    Path(definition_id): Path<String>,
    Json(request): Json<RunRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let definition = state
        .db
        .task_get_definition(&definition_id)
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?
        .ok_or_else(|| {
            error(
                StatusCode::NOT_FOUND,
                "definition_not_found",
                "task definition not found",
            )
        })?;
    if !definition.enabled {
        return Err(error(
            StatusCode::CONFLICT,
            "definition_disabled",
            "task definition is disabled",
        ));
    }
    if definition.kind == TaskKind::ActivitySummary {
        return Err(error(
            StatusCode::CONFLICT,
            "activity_summary_managed",
            "活动总结由系统活动入口按时间范围运行，不能直接从统一任务目录启动",
        ));
    }
    if definition.origin == TaskOrigin::User && definition.kind == TaskKind::PipeRun {
        return Err(error(
            StatusCode::CONFLICT,
            "pipe_definition_managed",
            "用户任务必须通过 Pipe 运行入口启动，不能直接创建公共任务运行",
        ));
    }
    let input_hash = if request.input_hash.is_empty() {
        let refs = request.input_refs.to_string();
        screenpipe_db::fingerprint(&[refs.as_str()])
    } else {
        request.input_hash
    };
    if let Some(kind) = knowledge_kind_for_task(definition.kind) {
        let scope_key = request.input_refs.get("scope_key").and_then(Value::as_str);
        let deadline = request
            .deadline
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc));
        let payload = request.input_refs.to_string();
        let (job_id, created) = state
            .db
            .knowledge_enqueue_job(
                kind,
                scope_key,
                Some(&input_hash),
                Some(&payload),
                deadline,
                None,
            )
            .await
            .map_err(|e| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "task_storage",
                    e.to_string(),
                )
            })?;
        return Ok(Json(json!({
            "run_id": format!("brain-job-{job_id}"),
            "created": created,
            "compatibility": "brain_jobs",
        })));
    }
    let (run_id, created) = state
        .db
        .task_start_run(&TaskRunRequest {
            definition_id: Some(definition_id),
            definition_revision: definition.config_revision,
            root_run_id: None,
            parent_run_id: None,
            retry_of: None,
            trigger_key: request.trigger_key,
            input_hash,
            input_refs: request.input_refs,
            config_snapshot: request.config_snapshot,
            priority: request
                .priority
                .unwrap_or_else(|| task_priority(definition.kind)),
            not_before: None,
            deadline: request.deadline,
            owner_generation: definition.owner_generation,
        })
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?;
    Ok(Json(json!({ "run_id": run_id, "created": created })))
}

async fn list_runs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = state
        .db
        .task_list_run_summaries(query.limit)
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?;
    Ok(Json(json!({
        "runs": rows.into_iter().map(|(run_id, definition_id, state, root_run_id, revision)| json!({
            "run_id": run_id,
            "definition_id": definition_id,
            "state": state,
            "root_run_id": root_run_id,
            "revision": revision,
        })).collect::<Vec<_>>()
    })))
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let run = state
        .db
        .task_get_run(&run_id)
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "run_not_found", "task run not found"))?;
    Ok(Json(json!({ "run": run })))
}

async fn list_events(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
    Query(query): Query<EventQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let events = state
        .db
        .task_list_events(&run_id, query.after_seq, query.limit)
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?;
    Ok(Json(json!({ "events": events })))
}

async fn control_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
    Json(request): Json<ControlRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let changed = state
        .db
        .task_control(&run_id, request.expected_revision, request.control)
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?;
    if !changed {
        return Err(error(
            StatusCode::CONFLICT,
            "revision_conflict",
            "run revision or state changed",
        ));
    }
    let run = state
        .db
        .task_get_run(&run_id)
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                e.to_string(),
            )
        })?
        .ok_or_else(|| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_storage",
                "run disappeared",
            )
        })?;
    Ok(Json(
        json!({ "run_id": run_id, "accepted": true, "run": run }),
    ))
}

#[allow(dead_code)]
fn _task_state_is_terminal(state: TaskState) -> bool {
    matches!(
        state,
        TaskState::Succeeded
            | TaskState::Failed
            | TaskState::TimedOut
            | TaskState::Cancelled
            | TaskState::NeedsAttention
    )
}
