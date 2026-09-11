// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Durable storage for the unified task contract.
//!
//! Every mutating method uses `begin_immediate_with_retry`, so task state,
//! attempts and ordered events share the same SQLite writer as Knowledge, Pipes,
//! and capture data. Payloads are references only; arbitrary model/office
//! bodies never enter the task event table.

use super::*;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

pub use screenpipe_core::tasks::{
    TaskAttempt, TaskControl, TaskDefinition, TaskEvent, TaskKind, TaskLegacyMapping, TaskOrigin,
    TaskOwnerState, TaskResourceClass, TaskRetryPolicy, TaskRun, TaskState, TaskTrigger,
};

#[derive(Clone, Debug)]
pub struct TaskRunRequest {
    pub definition_id: Option<String>,
    pub definition_revision: String,
    pub root_run_id: Option<String>,
    pub parent_run_id: Option<String>,
    pub retry_of: Option<String>,
    pub trigger_key: String,
    pub input_hash: String,
    pub input_refs: Value,
    pub config_snapshot: Value,
    pub priority: i32,
    pub not_before: Option<String>,
    pub deadline: Option<String>,
    pub owner_generation: i64,
}

#[derive(Clone, Debug)]
pub struct ClaimedTaskRun {
    pub run: TaskRun,
    pub attempt: TaskAttempt,
}

#[derive(sqlx::FromRow)]
struct TaskClaimRow {
    run_id: String,
    definition_id: Option<String>,
    definition_revision: String,
    root_run_id: String,
    parent_run_id: Option<String>,
    retry_of: Option<String>,
    trigger_key: String,
    input_hash: String,
    input_refs: String,
    config_snapshot: String,
    priority: i32,
    owner_generation: i64,
    revision: i64,
    not_before: Option<String>,
    deadline: Option<String>,
    cursor: Option<String>,
    output_refs: String,
}

#[derive(sqlx::FromRow)]
struct TaskRunRow {
    run_id: String,
    definition_id: Option<String>,
    definition_revision: String,
    root_run_id: String,
    parent_run_id: Option<String>,
    retry_of: Option<String>,
    trigger_key: String,
    input_hash: String,
    input_refs: String,
    config_snapshot: String,
    revision: i64,
    priority: i32,
    not_before: Option<String>,
    deadline: Option<String>,
    cursor: Option<String>,
    lease_owner: Option<String>,
    lease_token: Option<String>,
    lease_expires_at: Option<String>,
    owner_generation: i64,
    output_refs: String,
    error_code: Option<String>,
    state: String,
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

fn parse_json(value: Option<String>) -> Value {
    value
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_else(|| Value::Object(Default::default()))
}

fn active_state(state: &str) -> bool {
    matches!(state, "queued" | "running" | "paused" | "cancelling")
}

impl DatabaseManager {
    pub async fn task_upsert_definition(
        &self,
        definition: &TaskDefinition,
        expected_revision: Option<i64>,
    ) -> Result<bool, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let current: Option<i64> =
            sqlx::query_scalar("SELECT revision FROM task_definitions WHERE definition_id = ?1")
                .bind(&definition.definition_id)
                .fetch_optional(&mut **tx.conn())
                .await?;
        if let Some(expected) = expected_revision {
            if current != Some(expected) {
                tx.commit().await?;
                return Ok(false);
            }
        }
        let next_revision = current.map_or(1, |value| value + 1);
        sqlx::query(
            "INSERT INTO task_definitions (definition_id,kind,origin,schema_version,config_revision,config_ref,trigger_json,enabled,resource_class,retry_policy_json,model_binding_policy,revision,owner_generation,migration_state) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14) \
             ON CONFLICT(definition_id) DO UPDATE SET kind=?2,origin=?3,schema_version=?4,config_revision=?5,config_ref=?6,trigger_json=?7,enabled=?8,resource_class=?9,retry_policy_json=?10,model_binding_policy=?11,revision=?12,owner_generation=?13,migration_state=?14,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(&definition.definition_id)
        .bind(definition.kind.as_str())
        .bind(definition.origin.as_str())
        .bind(definition.schema_version as i64)
        .bind(&definition.config_revision)
        .bind(&definition.config_ref)
        .bind(json(&serde_json::to_value(&definition.trigger).unwrap_or_default()))
        .bind(definition.enabled)
        .bind(definition.resource_class.as_str())
        .bind(json(&serde_json::to_value(&definition.retry_policy).unwrap_or_default()))
        .bind(&definition.model_binding_policy)
        .bind(next_revision)
        .bind(definition.owner_generation)
        .bind(&definition.migration_state)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn task_get_definition(
        &self,
        definition_id: &str,
    ) -> Result<Option<TaskDefinition>, SqlxError> {
        let row: Option<(String,String,String,i64,String,Option<String>,String,i64,String,String,Option<String>,i64,i64,String)> = sqlx::query_as(
            "SELECT definition_id,kind,origin,schema_version,config_revision,config_ref,trigger_json,enabled,resource_class,retry_policy_json,model_binding_policy,revision,owner_generation,migration_state FROM task_definitions WHERE definition_id=?1",
        ).bind(definition_id).fetch_optional(&self.pool).await?;
        Ok(row.and_then(
            |(
                id,
                kind,
                origin,
                schema,
                config_revision,
                config_ref,
                trigger,
                enabled,
                resource,
                retry,
                model,
                revision,
                generation,
                migration,
            )| {
                Some(TaskDefinition {
                    definition_id: id,
                    kind: kind.parse().ok()?,
                    origin: match origin.as_str() {
                        "builtin" => TaskOrigin::Builtin,
                        "user" => TaskOrigin::User,
                        "connection" => TaskOrigin::Connection,
                        _ => return None,
                    },
                    schema_version: schema as u32,
                    config_revision,
                    config_ref,
                    trigger: serde_json::from_str(&trigger).ok()?,
                    enabled: enabled != 0,
                    resource_class: match resource.as_str() {
                        "interactive" => TaskResourceClass::Interactive,
                        "extract" => TaskResourceClass::Extract,
                        "backfill" => TaskResourceClass::Backfill,
                        "office_io" => TaskResourceClass::OfficeIo,
                        "user_pipe" => TaskResourceClass::UserPipe,
                        _ => return None,
                    },
                    retry_policy: serde_json::from_str(&retry).ok()?,
                    model_binding_policy: model,
                    revision,
                    owner_generation: generation,
                    migration_state: migration,
                })
            },
        ))
    }

    pub async fn task_list_definitions(&self) -> Result<Vec<TaskDefinition>, SqlxError> {
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT definition_id FROM task_definitions ORDER BY definition_id")
                .fetch_all(&self.pool)
                .await?;
        let mut result = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(definition) = self.task_get_definition(&id).await? {
                result.push(definition);
            }
        }
        Ok(result)
    }

    pub async fn task_start_run(
        &self,
        request: &TaskRunRequest,
    ) -> Result<(String, bool), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT run_id FROM task_runs WHERE definition_id IS ?1 AND input_hash=?2 AND trigger_key=?3 AND state IN ('queued','running','paused','cancelling') LIMIT 1",
        ).bind(&request.definition_id).bind(&request.input_hash).bind(&request.trigger_key).fetch_optional(&mut **tx.conn()).await?;
        if let Some(run_id) = existing {
            tx.commit().await?;
            return Ok((run_id, false));
        }
        let run_id = Uuid::new_v4().to_string();
        let root = request
            .root_run_id
            .clone()
            .unwrap_or_else(|| run_id.clone());
        sqlx::query(
            "INSERT INTO task_runs (run_id,definition_id,definition_revision,root_run_id,parent_run_id,retry_of,trigger_key,input_hash,input_refs,config_snapshot,state,priority,not_before,deadline,owner_generation) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'queued',?11,?12,?13,?14)",
        ).bind(&run_id).bind(&request.definition_id).bind(&request.definition_revision).bind(&root).bind(&request.parent_run_id).bind(&request.retry_of).bind(&request.trigger_key).bind(&request.input_hash).bind(json(&request.input_refs)).bind(json(&request.config_snapshot)).bind(request.priority).bind(&request.not_before).bind(&request.deadline).bind(request.owner_generation).execute(&mut **tx.conn()).await?;
        let seq = 1i64;
        sqlx::query("INSERT INTO task_events (run_id,seq,phase,event_type,safe_metadata) VALUES (?1,?2,'queued','run_queued','{}')").bind(&run_id).bind(seq).execute(&mut **tx.conn()).await?;
        tx.commit().await?;
        Ok((run_id, true))
    }

    pub async fn task_claim_next(
        &self,
        owner: &str,
        lease_seconds: u64,
    ) -> Result<Option<ClaimedTaskRun>, SqlxError> {
        self.task_claim_next_for_definitions(owner, lease_seconds, &[])
            .await
    }

    /// Claim the next queued run owned by one of the supplied definition IDs.
    /// An empty filter retains the general-purpose task queue behavior; runtime
    /// owners should pass their exact definition set so they cannot steal a
    /// run belonging to another handler family.
    pub async fn task_claim_next_for_definitions(
        &self,
        owner: &str,
        lease_seconds: u64,
        definition_ids: &[&str],
    ) -> Result<Option<ClaimedTaskRun>, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let now = now();
        let filter = if definition_ids.is_empty() {
            String::new()
        } else {
            let placeholders = (0..definition_ids.len())
                .map(|index| format!("?{}", index + 2))
                .collect::<Vec<_>>()
                .join(",");
            format!(" AND definition_id IN ({placeholders})")
        };
        let sql = format!(
            "SELECT run_id,definition_id,definition_revision,root_run_id,parent_run_id,retry_of,trigger_key,input_hash,input_refs,config_snapshot,priority,owner_generation,revision,not_before,deadline,cursor,output_refs FROM task_runs WHERE state='queued' AND (not_before IS NULL OR not_before <= ?1){filter} ORDER BY priority,created_at LIMIT 1"
        );
        let mut query = sqlx::query_as::<_, TaskClaimRow>(sqlx::AssertSqlSafe(sql)).bind(&now);
        for definition_id in definition_ids {
            query = query.bind(definition_id);
        }
        let row: Option<TaskClaimRow> = query.fetch_optional(&mut **tx.conn()).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let TaskClaimRow {
            run_id,
            definition_id,
            definition_revision,
            root_run_id,
            parent_run_id,
            retry_of,
            trigger_key,
            input_hash,
            input_refs,
            config_snapshot,
            priority,
            owner_generation,
            revision,
            not_before,
            deadline,
            cursor,
            output_refs,
        } = row;
        let token = Uuid::new_v4().to_string();
        let expiry = (Utc::now() + chrono::Duration::seconds(lease_seconds as i64)).to_rfc3339();
        let changed = sqlx::query("UPDATE task_runs SET state='running',lease_owner=?1,lease_token=?2,lease_expires_at=?3,revision=revision+1,updated_at=?4 WHERE run_id=?5 AND state='queued'").bind(owner).bind(&token).bind(&expiry).bind(&now).bind(&run_id).execute(&mut **tx.conn()).await?.rows_affected();
        if changed == 0 {
            tx.commit().await?;
            return Ok(None);
        }
        let attempt_id = Uuid::new_v4().to_string();
        let attempt_no: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(attempt_no),0)+1 FROM task_attempts WHERE run_id=?1",
        )
        .bind(&run_id)
        .fetch_one(&mut **tx.conn())
        .await?;
        sqlx::query("INSERT INTO task_attempts (attempt_id,run_id,attempt_no,started_at,owner_generation,lease_token) VALUES (?1,?2,?3,?4,?5,?6)").bind(&attempt_id).bind(&run_id).bind(attempt_no).bind(&now).bind(owner_generation).bind(&token).execute(&mut **tx.conn()).await?;
        let seq: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(seq),0)+1 FROM task_events WHERE run_id=?1")
                .bind(&run_id)
                .fetch_one(&mut **tx.conn())
                .await?;
        sqlx::query("INSERT INTO task_events (run_id,seq,attempt_id,phase,event_type,safe_metadata) VALUES (?1,?2,?3,'running','run_claimed',?4)").bind(&run_id).bind(seq).bind(&attempt_id).bind(json(&serde_json::json!({"owner":owner}))).execute(&mut **tx.conn()).await?;
        tx.commit().await?;
        Ok(Some(ClaimedTaskRun {
            run: TaskRun {
                run_id: run_id.clone(),
                definition_id,
                definition_revision,
                root_run_id,
                parent_run_id,
                retry_of,
                trigger_key,
                input_hash,
                input_refs: parse_json(Some(input_refs)),
                config_snapshot: parse_json(Some(config_snapshot)),
                state: TaskState::Running,
                revision: revision + 1,
                priority,
                not_before,
                deadline,
                cursor,
                lease_owner: Some(owner.to_string()),
                lease_token: Some(token.clone()),
                lease_expires_at: Some(expiry),
                owner_generation,
                output_refs: parse_json(Some(output_refs)),
                error_code: None,
            },
            attempt: TaskAttempt {
                attempt_id,
                run_id,
                attempt_no: attempt_no as u32,
                started_at: Some(now),
                finished_at: None,
                runtime_binding: None,
                model_calls_used: 0,
                retry_reason: None,
                outcome: None,
            },
        }))
    }

    pub async fn task_claim_run(
        &self,
        run_id: &str,
        owner: &str,
        lease_seconds: u64,
    ) -> Result<Option<ClaimedTaskRun>, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let now = now();
        let row: Option<TaskClaimRow> = sqlx::query_as(
            "SELECT run_id,definition_id,definition_revision,root_run_id,parent_run_id,retry_of,trigger_key,input_hash,input_refs,config_snapshot,priority,owner_generation,revision,not_before,deadline,cursor,output_refs FROM task_runs WHERE run_id=?1 AND state='queued' AND (not_before IS NULL OR not_before <= ?2)",
        )
        .bind(run_id)
        .bind(&now)
        .fetch_optional(&mut **tx.conn())
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let TaskClaimRow {
            run_id,
            definition_id,
            definition_revision,
            root_run_id,
            parent_run_id,
            retry_of,
            trigger_key,
            input_hash,
            input_refs,
            config_snapshot,
            priority,
            owner_generation,
            revision,
            not_before,
            deadline,
            cursor,
            output_refs,
        } = row;
        let token = Uuid::new_v4().to_string();
        let expiry = (Utc::now() + chrono::Duration::seconds(lease_seconds as i64)).to_rfc3339();
        let changed = sqlx::query(
            "UPDATE task_runs SET state='running',lease_owner=?1,lease_token=?2,lease_expires_at=?3,revision=revision+1,updated_at=?4 WHERE run_id=?5 AND state='queued'",
        )
        .bind(owner)
        .bind(&token)
        .bind(&expiry)
        .bind(&now)
        .bind(&run_id)
        .execute(&mut **tx.conn())
        .await?
        .rows_affected();
        if changed == 0 {
            tx.commit().await?;
            return Ok(None);
        }
        let attempt_id = Uuid::new_v4().to_string();
        let attempt_no: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(attempt_no),0)+1 FROM task_attempts WHERE run_id=?1",
        )
        .bind(&run_id)
        .fetch_one(&mut **tx.conn())
        .await?;
        sqlx::query(
            "INSERT INTO task_attempts (attempt_id,run_id,attempt_no,started_at,owner_generation,lease_token) VALUES (?1,?2,?3,?4,?5,?6)",
        )
        .bind(&attempt_id)
        .bind(&run_id)
        .bind(attempt_no)
        .bind(&now)
        .bind(owner_generation)
        .bind(&token)
        .execute(&mut **tx.conn())
        .await?;
        let seq: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(seq),0)+1 FROM task_events WHERE run_id=?1")
                .bind(&run_id)
                .fetch_one(&mut **tx.conn())
                .await?;
        sqlx::query(
            "INSERT INTO task_events (run_id,seq,attempt_id,phase,event_type,safe_metadata) VALUES (?1,?2,?3,'running','run_claimed',?4)",
        )
        .bind(&run_id)
        .bind(&seq)
        .bind(&attempt_id)
        .bind(json(&serde_json::json!({"owner": owner})))
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(Some(ClaimedTaskRun {
            run: TaskRun {
                run_id: run_id.clone(),
                definition_id,
                definition_revision,
                root_run_id,
                parent_run_id,
                retry_of,
                trigger_key,
                input_hash,
                input_refs: parse_json(Some(input_refs)),
                config_snapshot: parse_json(Some(config_snapshot)),
                state: TaskState::Running,
                revision: revision + 1,
                priority,
                not_before,
                deadline,
                cursor,
                lease_owner: Some(owner.to_string()),
                lease_token: Some(token.clone()),
                lease_expires_at: Some(expiry),
                owner_generation,
                output_refs: parse_json(Some(output_refs)),
                error_code: None,
            },
            attempt: TaskAttempt {
                attempt_id,
                run_id,
                attempt_no: attempt_no as u32,
                started_at: Some(now),
                finished_at: None,
                runtime_binding: None,
                model_calls_used: 0,
                retry_reason: None,
                outcome: None,
            },
        }))
    }

    pub async fn task_heartbeat(
        &self,
        run_id: &str,
        token: &str,
        lease_seconds: u64,
    ) -> Result<bool, SqlxError> {
        let expiry = (Utc::now() + chrono::Duration::seconds(lease_seconds as i64)).to_rfc3339();
        let mut tx = self.begin_immediate_with_retry().await?;
        let changed = sqlx::query("UPDATE task_runs SET lease_expires_at=?1,revision=revision+1,updated_at=?2 WHERE run_id=?3 AND lease_token=?4 AND state='running'").bind(expiry).bind(now()).bind(run_id).bind(token).execute(&mut **tx.conn()).await?.rows_affected();
        tx.commit().await?;
        Ok(changed > 0)
    }

    pub async fn task_append_event(&self, event: &TaskEvent) -> Result<i64, SqlxError> {
        let metadata = json(&event.safe_metadata);
        let output_refs = json(&event.output_refs);
        if metadata.len()
            + output_refs.len()
            + event.payload_ref.as_deref().unwrap_or_default().len()
            > screenpipe_core::tasks::TASK_EVENT_MAX_BYTES
        {
            return Err(SqlxError::Protocol(
                "task event exceeds the 64 KiB contract".into(),
            ));
        }
        let mut tx = self.begin_immediate_with_retry().await?;
        let requested_seq = event.seq;
        let seq = if requested_seq > 0 {
            requested_seq
        } else {
            sqlx::query_scalar("SELECT COALESCE(MAX(seq),0)+1 FROM task_events WHERE run_id=?1")
                .bind(&event.run_id)
                .fetch_one(&mut **tx.conn())
                .await?
        };
        sqlx::query("INSERT OR IGNORE INTO task_events (run_id,seq,attempt_id,phase,event_type,timestamp,safe_metadata,payload_ref,output_refs) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)").bind(&event.run_id).bind(seq).bind(&event.attempt_id).bind(&event.phase).bind(&event.event_type).bind(&event.timestamp).bind(metadata).bind(&event.payload_ref).bind(output_refs).execute(&mut **tx.conn()).await?;
        sqlx::query(
            "DELETE FROM task_events WHERE run_id=?1 AND seq <= (SELECT COALESCE(MAX(seq),0)-?2 FROM task_events WHERE run_id=?1)",
        )
        .bind(&event.run_id)
        .bind(screenpipe_core::tasks::TASK_MESSAGE_RETENTION_LIMIT as i64)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(seq)
    }

    pub async fn task_finish_run(
        &self,
        run_id: &str,
        token: &str,
        state: TaskState,
        output_refs: &Value,
        error_code: Option<&str>,
    ) -> Result<bool, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let now = now();
        let changed = sqlx::query("UPDATE task_runs SET state=?1,output_refs=?2,error_code=?3,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,revision=revision+1,updated_at=?4 WHERE run_id=?5 AND lease_token=?6 AND state IN ('running','cancelling')").bind(state.as_str()).bind(json(output_refs)).bind(error_code).bind(&now).bind(run_id).bind(token).execute(&mut **tx.conn()).await?.rows_affected();
        if changed > 0 {
            sqlx::query("UPDATE task_attempts SET finished_at=?1,outcome=?2,lease_token=NULL WHERE run_id=?3 AND lease_token=?4 AND finished_at IS NULL")
                .bind(&now)
                .bind(state.as_str())
                .bind(run_id)
                .bind(token)
                .execute(&mut **tx.conn())
                .await?;
            let seq: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(seq),0)+1 FROM task_events WHERE run_id=?1",
            )
            .bind(run_id)
            .fetch_one(&mut **tx.conn())
            .await?;
            sqlx::query("INSERT INTO task_events (run_id,seq,phase,event_type,safe_metadata,output_refs) VALUES (?1,?2,'completed','run_finished',?3,?4)").bind(run_id).bind(seq).bind(json(&serde_json::json!({"state":state.as_str()}))).bind(json(output_refs)).execute(&mut **tx.conn()).await?;
        }
        tx.commit().await?;
        Ok(changed > 0)
    }

    pub async fn task_control(
        &self,
        run_id: &str,
        expected_revision: i64,
        control: TaskControl,
    ) -> Result<bool, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let current: Option<(String, i64)> =
            sqlx::query_as("SELECT state,revision FROM task_runs WHERE run_id=?1")
                .bind(run_id)
                .fetch_optional(&mut **tx.conn())
                .await?;
        let Some((state, revision)) = current else {
            tx.commit().await?;
            return Ok(false);
        };
        if revision != expected_revision {
            tx.commit().await?;
            return Ok(false);
        }
        let target = match (&control, state.as_str()) {
            (TaskControl::Pause, current) if active_state(current) => "paused",
            (TaskControl::Resume, "paused") => "queued",
            (TaskControl::Cancel, "running") => "cancelling",
            (TaskControl::Cancel, "queued" | "paused" | "cancelling") => "cancelled",
            (TaskControl::Retry, "failed" | "timed_out" | "cancelled" | "needs_attention") => {
                "queued"
            }
            _ => {
                tx.commit().await?;
                return Ok(false);
            }
        };
        let changed = sqlx::query("UPDATE task_runs SET state=?1,revision=revision+1,updated_at=?2 WHERE run_id=?3 AND revision=?4").bind(target).bind(now()).bind(run_id).bind(expected_revision).execute(&mut **tx.conn()).await?.rows_affected();
        if changed == 0 {
            tx.commit().await?;
            return Ok(false);
        }
        // Knowledge's public run is temporarily backed by a compatibility row.
        // Keep queued controls in the same transaction so a cancelled or
        // retried public run cannot leave the legacy dedupe row stuck.
        if let Some(job_id) = run_id
            .strip_prefix("brain-job-")
            .and_then(|value| value.parse::<i64>().ok())
        {
            match (&control, state.as_str(), target) {
                (TaskControl::Pause, "running", "paused") => {
                    // The worker still owns the legacy lease. It observes the
                    // paused public state and closes both rows at a safe
                    // handler boundary via knowledge_pause_claimed_job.
                }
                (TaskControl::Pause, _, "paused") => {
                    sqlx::query("UPDATE knowledge_jobs SET state='paused',updated_at=?1 WHERE id=?2 AND state IN ('pending','paused')")
                        .bind(now()).bind(job_id).execute(&mut **tx.conn()).await?;
                }
                (TaskControl::Resume, "paused", "queued") => {
                    sqlx::query("UPDATE knowledge_jobs SET state='pending',not_before=NULL,updated_at=?1 WHERE id=?2 AND state='paused'")
                        .bind(now()).bind(job_id).execute(&mut **tx.conn()).await?;
                }
                (TaskControl::Cancel, "running", "cancelling") => {
                    // The active worker owns the lease and will commit the
                    // terminal cancellation with its token.
                }
                (TaskControl::Cancel, _, "cancelled") => {
                    sqlx::query("UPDATE knowledge_jobs SET state='cancelled',last_error_code='user_cancelled',lease_token=NULL,lease_expires_at=NULL,updated_at=?1 WHERE id=?2 AND state IN ('pending','paused')")
                        .bind(now()).bind(job_id).execute(&mut **tx.conn()).await?;
                }
                (TaskControl::Retry, _, "queued") => {
                    sqlx::query("UPDATE knowledge_jobs SET state='pending',attempts=0,model_calls=0,not_before=NULL,lease_token=NULL,lease_expires_at=NULL,updated_at=?1 WHERE id=?2 AND state IN ('failed','cancelled','paused')")
                        .bind(now()).bind(job_id).execute(&mut **tx.conn()).await?;
                }
                _ => {}
            }
        }
        let seq: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(seq),0)+1 FROM task_events WHERE run_id=?1")
                .bind(run_id)
                .fetch_one(&mut **tx.conn())
                .await?;
        sqlx::query("INSERT INTO task_events (run_id,seq,phase,event_type,safe_metadata) VALUES (?1,?2,?3,?4,?5)").bind(run_id).bind(seq).bind(target).bind("control").bind(json(&serde_json::json!({"control":control}))).execute(&mut **tx.conn()).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn task_get_run(&self, run_id: &str) -> Result<Option<TaskRun>, SqlxError> {
        let row: Option<TaskRunRow> = sqlx::query_as("SELECT run_id,definition_id,definition_revision,root_run_id,parent_run_id,retry_of,trigger_key,input_hash,input_refs,config_snapshot,revision,priority,not_before,deadline,cursor,lease_owner,lease_token,lease_expires_at,owner_generation,output_refs,error_code,state FROM task_runs WHERE run_id=?1").bind(run_id).fetch_optional(&self.pool).await?;
        Ok(row.map(|row| TaskRun {
            run_id: row.run_id,
            definition_id: row.definition_id,
            definition_revision: row.definition_revision,
            root_run_id: row.root_run_id,
            parent_run_id: row.parent_run_id,
            retry_of: row.retry_of,
            trigger_key: row.trigger_key,
            input_hash: row.input_hash,
            input_refs: parse_json(Some(row.input_refs)),
            config_snapshot: parse_json(Some(row.config_snapshot)),
            state: row.state.parse().unwrap_or(TaskState::Failed),
            revision: row.revision,
            priority: row.priority,
            not_before: row.not_before,
            deadline: row.deadline,
            cursor: row.cursor,
            lease_owner: row.lease_owner,
            lease_token: row.lease_token,
            lease_expires_at: row.lease_expires_at,
            owner_generation: row.owner_generation,
            output_refs: parse_json(Some(row.output_refs)),
            error_code: row.error_code,
        }))
    }

    pub async fn task_list_run_summaries(
        &self,
        limit: u32,
    ) -> Result<Vec<(String, Option<String>, String, String, i64)>, SqlxError> {
        sqlx::query_as(
            "SELECT run_id,definition_id,state,root_run_id,revision FROM task_runs WHERE root_run_id=run_id ORDER BY updated_at DESC LIMIT ?1",
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await
    }

    pub async fn task_list_events(
        &self,
        run_id: &str,
        after_seq: i64,
        limit: u32,
    ) -> Result<Vec<TaskEvent>, SqlxError> {
        let rows: Vec<(String,i64,Option<String>,String,String,String,String,Option<String>,String)> = sqlx::query_as("SELECT run_id,seq,attempt_id,phase,event_type,timestamp,safe_metadata,payload_ref,output_refs FROM task_events WHERE run_id=?1 AND seq>?2 ORDER BY seq LIMIT ?3").bind(run_id).bind(after_seq).bind(limit.clamp(1,1000)).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    run_id,
                    seq,
                    attempt_id,
                    phase,
                    event_type,
                    timestamp,
                    safe_metadata,
                    payload_ref,
                    output_refs,
                )| TaskEvent {
                    run_id,
                    seq,
                    attempt_id,
                    phase,
                    event_type,
                    timestamp,
                    safe_metadata: parse_json(Some(safe_metadata)),
                    payload_ref,
                    output_refs: parse_json(Some(output_refs)),
                },
            )
            .collect())
    }

    pub async fn task_reap_expired(&self) -> Result<u64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let now = now();
        let expired: Vec<(String, String)> = sqlx::query_as("SELECT run_id, lease_token FROM task_runs WHERE state='running' AND lease_expires_at IS NOT NULL AND lease_expires_at < ?1")
            .bind(&now)
            .fetch_all(&mut **tx.conn())
            .await?;
        let result = sqlx::query("UPDATE task_runs SET state='queued',lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,revision=revision+1,updated_at=?1 WHERE state='running' AND lease_expires_at IS NOT NULL AND lease_expires_at < ?1").bind(&now).execute(&mut **tx.conn()).await?;
        for (run_id, token) in &expired {
            sqlx::query("UPDATE task_attempts SET finished_at=?1,outcome='lease_expired',lease_token=NULL WHERE run_id=?2 AND lease_token=?3 AND finished_at IS NULL")
                .bind(&now).bind(run_id).bind(token).execute(&mut **tx.conn()).await?;
            let seq: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(seq),0)+1 FROM task_events WHERE run_id=?1",
            )
            .bind(run_id)
            .fetch_one(&mut **tx.conn())
            .await?;
            sqlx::query("INSERT INTO task_events (run_id,seq,phase,event_type,safe_metadata) VALUES (?1,?2,'queued','lease_expired',?)")
                .bind(run_id).bind(seq).bind(json(&serde_json::json!({"requeued": true}))).execute(&mut **tx.conn()).await?;
        }
        tx.commit().await?;
        Ok(result.rows_affected())
    }

    /// Redact task snapshots and event references before a deletion wave can
    /// reopen reads. Task rows intentionally contain only identifiers, but
    /// those identifiers still must not point back to erased source content.
    pub async fn task_redact_source_refs(&self, source_uids: &[String]) -> Result<u64, SqlxError> {
        if source_uids.is_empty() {
            return Ok(0);
        }
        let mut tx = self.begin_immediate_with_retry().await?;
        let rows: Vec<(String, String, String, String)> =
            sqlx::query_as("SELECT run_id,input_refs,config_snapshot,output_refs FROM task_runs")
                .fetch_all(&mut **tx.conn())
                .await?;
        let mut changed = 0;
        for (run_id, input, config, output) in rows {
            let mut values = [
                parse_json(Some(input)),
                parse_json(Some(config)),
                parse_json(Some(output)),
            ];
            let mut touched = false;
            for value in &mut values {
                touched |= redact_json(value, source_uids);
            }
            if touched {
                sqlx::query("UPDATE task_runs SET input_refs=?1,config_snapshot=?2,output_refs=?3,revision=revision+1,updated_at=?4 WHERE run_id=?5")
                    .bind(json(&values[0])).bind(json(&values[1])).bind(json(&values[2])).bind(&now()).bind(&run_id).execute(&mut **tx.conn()).await?;
                changed += 1;
            }
        }
        let events: Vec<(String, i64, String, Option<String>, String)> = sqlx::query_as(
            "SELECT run_id,seq,safe_metadata,payload_ref,output_refs FROM task_events",
        )
        .fetch_all(&mut **tx.conn())
        .await?;
        for (run_id, seq, metadata, payload_ref, output_refs) in events {
            let mut metadata_json = parse_json(Some(metadata));
            let mut output_json = parse_json(Some(output_refs));
            let mut touched = redact_json(&mut metadata_json, source_uids);
            touched |= redact_json(&mut output_json, source_uids);
            let payload_deleted = payload_ref
                .as_deref()
                .is_some_and(|payload| source_uids.iter().any(|uid| uid == payload));
            if touched || payload_deleted {
                sqlx::query(
                    "UPDATE task_events SET safe_metadata=?1,payload_ref=CASE WHEN ?2 THEN NULL ELSE payload_ref END,output_refs=?3 WHERE run_id=?4 AND seq=?5",
                )
                .bind(json(&metadata_json))
                .bind(payload_deleted)
                .bind(json(&output_json))
                .bind(run_id)
                .bind(seq)
                .execute(&mut **tx.conn())
                .await?;
            }
        }
        tx.commit().await?;
        Ok(changed)
    }

    pub async fn task_set_owner_state(
        &self,
        state: &TaskOwnerState,
        expected_generation: Option<i64>,
    ) -> Result<bool, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let current: Option<i64> =
            sqlx::query_scalar("SELECT owner_generation FROM task_owner_state WHERE kind=?1")
                .bind(&state.kind)
                .fetch_optional(&mut **tx.conn())
                .await?;
        if let Some(expected) = expected_generation {
            if current != Some(expected) {
                tx.commit().await?;
                return Ok(false);
            }
        }
        sqlx::query(
            "INSERT INTO task_owner_state(kind,owner_generation,migration_state,checkpoint) \
             VALUES (?1,?2,?3,?4) ON CONFLICT(kind) DO UPDATE SET \
             owner_generation=?2,migration_state=?3,checkpoint=?4,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(&state.kind)
        .bind(state.owner_generation)
        .bind(&state.migration_state)
        .bind(&state.checkpoint)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn task_get_owner_state(
        &self,
        kind: &str,
    ) -> Result<Option<TaskOwnerState>, SqlxError> {
        sqlx::query_as::<_, (String, i64, String, Option<String>)>(
            "SELECT kind,owner_generation,migration_state,checkpoint FROM task_owner_state WHERE kind=?1",
        )
        .bind(kind)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(|(kind, owner_generation, migration_state, checkpoint)| TaskOwnerState {
            kind,
            owner_generation,
            migration_state,
            checkpoint,
        }))
    }

    /// Atomically mark the queued definitions/runs for an owner generation
    /// after a runtime cutover. In-flight legacy work is intentionally left
    /// untouched; its lease must expire or finish before a new generation can
    /// claim it.
    pub async fn task_activate_owner_generation(
        &self,
        kind: &str,
        owner_generation: i64,
    ) -> Result<(), SqlxError> {
        let definition_ids: &[&str] = match kind {
            "knowledge" => &[
                "knowledge.extract",
                "knowledge.summarize",
                "knowledge.compile",
                "knowledge.backfill",
                "office.sync",
            ],
            _ => &[],
        };
        if definition_ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.begin_immediate_with_retry().await?;
        let placeholders = (0..definition_ids.len())
            .map(|index| format!("?{}", index + 2))
            .collect::<Vec<_>>()
            .join(",");
        let update_definitions = format!(
            "UPDATE task_definitions SET owner_generation=?1,migration_state='unified',updated_at=?{} WHERE definition_id IN ({placeholders})",
            definition_ids.len() + 2
        );
        let mut query = sqlx::query(sqlx::AssertSqlSafe(update_definitions)).bind(owner_generation);
        for definition_id in definition_ids {
            query = query.bind(definition_id);
        }
        query = query.bind(now());
        query.execute(&mut **tx.conn()).await?;

        let update_runs = format!(
            "UPDATE task_runs SET owner_generation=?1,revision=revision+1,updated_at=?{} WHERE state='queued' AND definition_id IN ({placeholders})",
            definition_ids.len() + 2
        );
        let mut query = sqlx::query(sqlx::AssertSqlSafe(update_runs)).bind(owner_generation);
        for definition_id in definition_ids {
            query = query.bind(definition_id);
        }
        query = query.bind(now());
        query.execute(&mut **tx.conn()).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn task_record_legacy_mapping(
        &self,
        mapping: &TaskLegacyMapping,
    ) -> Result<(), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "INSERT INTO task_legacy_map(legacy_namespace,legacy_id,definition_id,run_id) \
             VALUES (?1,?2,?3,?4) ON CONFLICT(legacy_namespace,legacy_id) DO UPDATE SET \
             definition_id=COALESCE(excluded.definition_id,task_legacy_map.definition_id), \
             run_id=COALESCE(excluded.run_id,task_legacy_map.run_id)",
        )
        .bind(&mapping.legacy_namespace)
        .bind(&mapping.legacy_id)
        .bind(&mapping.definition_id)
        .bind(&mapping.run_id)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn task_get_legacy_mapping(
        &self,
        namespace: &str,
        legacy_id: &str,
    ) -> Result<Option<TaskLegacyMapping>, SqlxError> {
        sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
            "SELECT legacy_namespace,legacy_id,definition_id,run_id FROM task_legacy_map WHERE legacy_namespace=?1 AND legacy_id=?2",
        )
        .bind(namespace)
        .bind(legacy_id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(|(legacy_namespace, legacy_id, definition_id, run_id)| TaskLegacyMapping {
            legacy_namespace,
            legacy_id,
            definition_id,
            run_id,
        }))
    }
}

fn redact_json(value: &mut Value, source_uids: &[String]) -> bool {
    match value {
        Value::String(text) if source_uids.iter().any(|uid| uid == text) => {
            *value = Value::String("[deleted-source]".into());
            true
        }
        Value::Array(items) => items
            .iter_mut()
            .map(|item| redact_json(item, source_uids))
            .any(|touched| touched),
        Value::Object(items) => items
            .values_mut()
            .map(|item| redact_json(item, source_uids))
            .any(|touched| touched),
        _ => false,
    }
}
