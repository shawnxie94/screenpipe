// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Contract tests for the durable unified task layer. All fixtures use an
//! isolated in-memory SQLite database and never contain model or office data.

use std::sync::Arc;

use screenpipe_db::{
    DatabaseManager, TaskControl, TaskDefinition, TaskEvent, TaskKind, TaskOrigin,
    TaskResourceClass, TaskRetryPolicy, TaskRunRequest, TaskTrigger, TaskState,
};
use serde_json::json;

async fn db() -> Arc<DatabaseManager> {
    Arc::new(
        DatabaseManager::new("sqlite::memory:", Default::default())
            .await
            .unwrap(),
    )
}

fn definition(id: &str) -> TaskDefinition {
    TaskDefinition {
        definition_id: id.to_string(),
        kind: TaskKind::BrainExtract,
        origin: TaskOrigin::Builtin,
        schema_version: 1,
        config_revision: "cfg-1".to_string(),
        config_ref: Some("brain:extract".to_string()),
        trigger: TaskTrigger { kind: "manual".to_string(), expression: None },
        enabled: true,
        resource_class: TaskResourceClass::Extract,
        retry_policy: TaskRetryPolicy { max_attempts: 3, network_retries: 1, invalid_output_retries: 1, backoff_ms: 1_000 },
        model_binding_policy: Some("selected_preset".to_string()),
        revision: 0,
        owner_generation: 1,
        migration_state: "unified".to_string(),
    }
}

#[tokio::test]
async fn definitions_use_revision_cas_and_runs_dedupe() {
    let db = db().await;
    assert!(db.task_upsert_definition(&definition("extract"), None).await.unwrap());
    let stored = db.task_get_definition("extract").await.unwrap().unwrap();
    assert_eq!(stored.revision, 1);
    let mut changed = definition("extract");
    changed.config_revision = "cfg-2".to_string();
    assert!(!db.task_upsert_definition(&changed, Some(99)).await.unwrap());
    assert!(db.task_upsert_definition(&changed, Some(1)).await.unwrap());
    assert_eq!(db.task_get_definition("extract").await.unwrap().unwrap().revision, 2);

    let request = TaskRunRequest {
        definition_id: Some("extract".to_string()),
        definition_revision: "cfg-2".to_string(),
        root_run_id: None,
        parent_run_id: None,
        retry_of: None,
        trigger_key: "manual:one".to_string(),
        input_hash: "input-1".to_string(),
        input_refs: json!({"source_uid":"synthetic-source"}),
        config_snapshot: json!({"preset_id":"fixture"}),
        priority: 10,
        not_before: None,
        deadline: None,
        owner_generation: 1,
    };
    let (run_id, created) = db.task_start_run(&request).await.unwrap();
    assert!(created);
    let (same_id, created_again) = db.task_start_run(&request).await.unwrap();
    assert_eq!(same_id, run_id);
    assert!(!created_again);
}

#[tokio::test]
async fn claim_heartbeat_finish_and_event_replay_are_lease_guarded() {
    let db = db().await;
    db.task_upsert_definition(&definition("extract"), None).await.unwrap();
    let (run_id, _) = db.task_start_run(&TaskRunRequest {
        definition_id: Some("extract".to_string()),
        definition_revision: "cfg-1".to_string(),
        root_run_id: None,
        parent_run_id: None,
        retry_of: None,
        trigger_key: "manual:two".to_string(),
        input_hash: "input-2".to_string(),
        input_refs: json!({"source_uid":"synthetic-source"}),
        config_snapshot: json!({"secret_ref":"keychain://fixture"}),
        priority: 10,
        not_before: None,
        deadline: None,
        owner_generation: 1,
    }).await.unwrap();
    let claimed = db.task_claim_next("worker-a", 30).await.unwrap().unwrap();
    assert_eq!(claimed.run.run_id, run_id);
    let token = claimed.run.lease_token.clone().unwrap();
    assert!(db.task_heartbeat(&run_id, &token, 30).await.unwrap());
    assert!(!db.task_heartbeat(&run_id, "stale-token", 30).await.unwrap());

    let seq = db.task_append_event(&TaskEvent {
        run_id: run_id.clone(),
        seq: 0,
        attempt_id: Some(claimed.attempt.attempt_id),
        phase: "processing".to_string(),
        event_type: "checkpoint".to_string(),
        timestamp: "2026-09-08T00:00:01Z".to_string(),
        safe_metadata: json!({"count":1}),
        payload_ref: Some("source:synthetic-source".to_string()),
        output_refs: json!({}),
    }).await.unwrap();
    assert_eq!(seq, 3);
    assert!(db.task_finish_run(&run_id, &token, TaskState::Succeeded, &json!({"work_unit_id":"wu-1"}), None).await.unwrap());
    assert!(!db.task_finish_run(&run_id, &token, TaskState::Succeeded, &json!({}), None).await.unwrap());
    let events = db.task_list_events(&run_id, 0, 10).await.unwrap();
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].event_type, "run_queued");
    assert_eq!(events.last().unwrap().event_type, "run_finished");
}

#[tokio::test]
async fn controls_are_cas_guarded_and_reap_returns_expired_runs() {
    let db = db().await;
    db.task_upsert_definition(&definition("extract"), None).await.unwrap();
    let (run_id, _) = db.task_start_run(&TaskRunRequest {
        definition_id: Some("extract".to_string()),
        definition_revision: "cfg-1".to_string(),
        root_run_id: None,
        parent_run_id: None,
        retry_of: None,
        trigger_key: "manual:three".to_string(),
        input_hash: "input-3".to_string(),
        input_refs: json!({}),
        config_snapshot: json!({}),
        priority: 10,
        not_before: None,
        deadline: None,
        owner_generation: 1,
    }).await.unwrap();
    assert!(db.task_control(&run_id, 1, TaskControl::Pause).await.unwrap());
    assert!(!db.task_control(&run_id, 1, TaskControl::Resume).await.unwrap());
    let run = db.task_get_run(&run_id).await.unwrap().unwrap();
    assert_eq!(run.state, TaskState::Paused);
    assert!(db.task_control(&run_id, run.revision, TaskControl::Resume).await.unwrap());
    let claimed = db.task_claim_next("worker-b", 0).await.unwrap().unwrap();
    assert_eq!(claimed.run.state, TaskState::Running);
    assert_eq!(db.task_reap_expired().await.unwrap(), 1);
    assert_eq!(db.task_get_run(&run_id).await.unwrap().unwrap().state, TaskState::Queued);
}
