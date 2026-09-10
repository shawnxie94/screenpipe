// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Compatibility/ownership checks for the Knowledge -> public-task migration.
//! These tests deliberately use an isolated SQLite database and never touch a
//! user's recording store.

use std::sync::Arc;

use screenpipe_db::{KnowledgeJobKind, DatabaseManager, TaskState};

async fn db() -> Arc<DatabaseManager> {
    Arc::new(
        DatabaseManager::new("sqlite::memory:", Default::default())
            .await
            .expect("isolated database"),
    )
}

#[tokio::test]
async fn knowledge_enqueue_claim_model_budget_and_completion_share_one_run() {
    let db = db().await;
    let (job_id, created) = db
        .knowledge_enqueue_job(
            KnowledgeJobKind::Extract,
            Some("scope-a"),
            Some("input-a"),
            Some("{\"interval\":\"synthetic\"}"),
            None,
            None,
        )
        .await
        .expect("enqueue");
    assert!(created);

    let mapping = db
        .task_get_legacy_mapping("brain_jobs", &job_id.to_string())
        .await
        .expect("mapping lookup")
        .expect("legacy mapping");
    let run_id = mapping.run_id.expect("public run id");
    assert_eq!(run_id, format!("brain-job-{job_id}"));
    assert_eq!(
        db.task_get_run(&run_id)
            .await
            .expect("run lookup")
            .expect("run")
            .state,
        TaskState::Queued
    );

    let claimed = db
        .knowledge_claim_next_job(&[KnowledgeJobKind::Extract], "migration-test", 30_000)
        .await
        .expect("claim")
        .expect("claimed job");
    assert_eq!(claimed.id, job_id);
    let token = claimed.lease_token.clone();
    let run = db
        .task_get_run(&run_id)
        .await
        .expect("running lookup")
        .expect("running run");
    assert_eq!(run.state, TaskState::Running);
    assert_eq!(run.lease_owner.as_deref(), Some("migration-test"));

    assert!(db
        .knowledge_job_model_call(job_id, &token)
        .await
        .expect("model call"));
    let attempt_calls: i64 = sqlx::query_scalar(
        "SELECT model_calls_used FROM task_attempts WHERE run_id=?1 AND finished_at IS NULL",
    )
    .bind(&run_id)
    .fetch_one(&db.pool)
    .await
    .expect("attempt counter");
    assert_eq!(attempt_calls, 1);

    assert!(db
        .knowledge_complete_job(job_id, &token, Some("wu-1"), Some("cursor-1"))
        .await
        .expect("complete"));
    let finished = db
        .task_get_run(&run_id)
        .await
        .expect("finished lookup")
        .expect("finished run");
    assert_eq!(finished.state, TaskState::Succeeded);
    assert_eq!(finished.cursor.as_deref(), Some("cursor-1"));
    assert_eq!(finished.output_refs["result_ref"], "wu-1");
    assert!(db
        .task_list_events(&run_id, 0, 20)
        .await
        .expect("events")
        .iter()
        .any(|event| event.event_type == "run_finished"));

    // A late result from the old owner cannot commit again after the lease
    // has been cleared by the public run completion transaction.
    assert!(!db
        .knowledge_complete_job(job_id, &token, Some("late"), None)
        .await
        .expect("late completion check"));
}

#[tokio::test]
async fn owner_generation_and_event_redaction_are_cas_and_deletion_safe() {
    let db = db().await;
    assert!(db
        .task_set_owner_state(
            &screenpipe_db::TaskOwnerState {
                kind: "knowledge".into(),
                owner_generation: 1,
                migration_state: "cutover_ready".into(),
                checkpoint: Some("job-7".into()),
            },
            None,
        )
        .await
        .expect("owner state"));
    assert!(!db
        .task_set_owner_state(
            &screenpipe_db::TaskOwnerState {
                kind: "knowledge".into(),
                owner_generation: 2,
                migration_state: "stale_writer".into(),
                checkpoint: None,
            },
            Some(0),
        )
        .await
        .expect("owner CAS"));

    let (run_id, _) = db
        .task_start_run(&screenpipe_db::TaskRunRequest {
            definition_id: None,
            definition_revision: "test".into(),
            root_run_id: None,
            parent_run_id: None,
            retry_of: None,
            trigger_key: "redaction".into(),
            input_hash: "redaction-input".into(),
            input_refs: serde_json::json!({"source_uid":"source-1"}),
            config_snapshot: serde_json::json!({}),
            priority: 1,
            not_before: None,
            deadline: None,
            owner_generation: 1,
        })
        .await
        .expect("run");
    db.task_append_event(&screenpipe_db::TaskEvent {
        run_id: run_id.clone(),
        seq: 0,
        attempt_id: None,
        phase: "running".into(),
        event_type: "source_used".into(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        safe_metadata: serde_json::json!({"source_uid":"source-1"}),
        payload_ref: Some("source-1".into()),
        output_refs: serde_json::json!({}),
    })
    .await
    .expect("event");
    assert_eq!(
        db.task_redact_source_refs(&["source-1".into()])
            .await
            .unwrap(),
        1
    );
    let run = db.task_get_run(&run_id).await.unwrap().unwrap();
    assert_eq!(run.input_refs["source_uid"], "[deleted-source]");
    let events = db.task_list_events(&run_id, 0, 10).await.unwrap();
    let source_event = events
        .iter()
        .find(|event| event.event_type == "source_used")
        .unwrap();
    assert!(source_event.payload_ref.is_none());
    assert_eq!(source_event.safe_metadata["source_uid"], "[deleted-source]");
}

#[tokio::test]
async fn public_task_claim_binds_the_legacy_knowledge_row_for_one_owner() {
    let db = db().await;
    let (job_id, created) = db
        .knowledge_enqueue_job(
            KnowledgeJobKind::OfficeSync,
            Some("feishu"),
            Some("office-input"),
            Some("{\"provider\":\"feishu\"}"),
            None,
            None,
        )
        .await
        .expect("enqueue");
    assert!(created);

    let claimed = db
        .task_claim_next_for_definitions("public-owner", 30, &["office.sync"])
        .await
        .expect("public task claim")
        .expect("task run");
    let token = claimed.run.lease_token.clone().expect("task lease");
    let bound = db
        .knowledge_bind_public_task(
            job_id,
            &claimed.run.run_id,
            KnowledgeJobKind::OfficeSync,
            "public-owner",
            &token,
            30_000,
        )
        .await
        .expect("bind")
        .expect("legacy row");
    assert_eq!(bound.lease_token, token);

    assert!(db
        .knowledge_job_model_call(job_id, &token)
        .await
        .expect("shared model counter"));
    assert!(db
        .knowledge_complete_job(job_id, &token, Some("office-result"), Some("cursor"))
        .await
        .expect("completion"));
    assert_eq!(
        db.task_get_run(&claimed.run.run_id)
            .await
            .expect("run")
            .expect("run row")
            .state,
        TaskState::Succeeded
    );
}

#[tokio::test]
async fn public_task_cancellation_closes_both_compatibility_rows() {
    let db = db().await;
    let (job_id, _) = db
        .knowledge_enqueue_job(
            KnowledgeJobKind::Extract,
            Some("scope-cancel"),
            Some("cancel-input"),
            Some("{}"),
            None,
            None,
        )
        .await
        .expect("enqueue");
    let claimed = db
        .task_claim_next_for_definitions("public-owner", 30, &["knowledge.extract"])
        .await
        .expect("claim")
        .expect("run");
    let token = claimed.run.lease_token.clone().expect("lease");
    db.knowledge_bind_public_task(
        job_id,
        &claimed.run.run_id,
        KnowledgeJobKind::Extract,
        "public-owner",
        &token,
        30_000,
    )
    .await
    .expect("bind")
    .expect("legacy row");

    let running = db
        .task_get_run(&claimed.run.run_id)
        .await
        .expect("run lookup")
        .expect("run");
    assert!(db
        .task_control(
            &claimed.run.run_id,
            running.revision,
            screenpipe_db::TaskControl::Cancel,
        )
        .await
        .expect("cancel request"));
    assert!(db
        .knowledge_cancel_claimed_job(job_id, &token, "user_cancelled")
        .await
        .expect("cancel compatibility job"));
    assert_eq!(
        db.task_get_run(&claimed.run.run_id)
            .await
            .expect("run lookup")
            .expect("run")
            .state,
        TaskState::Cancelled
    );
    assert_eq!(
        db.knowledge_get_job(job_id)
            .await
            .expect("job lookup")
            .expect("job")
            .state,
        "cancelled"
    );
}

#[tokio::test]
async fn office_scope_cancellation_does_not_cross_provider_boundaries() {
    let db = db().await;
    let (feishu_id, _) = db
        .knowledge_enqueue_job(
            KnowledgeJobKind::OfficeSync,
            Some("feishu"),
            Some("feishu-input"),
            Some("{\"provider\":\"feishu\"}"),
            None,
            None,
        )
        .await
        .expect("feishu enqueue");
    let (tencent_id, _) = db
        .knowledge_enqueue_job(
            KnowledgeJobKind::OfficeSync,
            Some("tencent-meeting"),
            Some("tencent-input"),
            Some("{\"provider\":\"tencent-meeting\"}"),
            None,
            None,
        )
        .await
        .expect("tencent enqueue");

    assert_eq!(
        db.knowledge_cancel_jobs_for_scope(KnowledgeJobKind::OfficeSync, "feishu", "paused_by_user",)
            .await
            .expect("scope cancellation"),
        1
    );
    assert_eq!(
        db.knowledge_get_job(feishu_id)
            .await
            .expect("feishu lookup")
            .expect("feishu row")
            .state,
        "cancelled"
    );
    assert_eq!(
        db.knowledge_get_job(tencent_id)
            .await
            .expect("tencent lookup")
            .expect("tencent row")
            .state,
        "pending"
    );
}

#[tokio::test]
async fn queued_public_controls_keep_legacy_knowledge_row_claimable() {
    let db = db().await;
    let (job_id, _) = db
        .knowledge_enqueue_job(
            KnowledgeJobKind::Extract,
            Some("scope-control"),
            Some("control-input"),
            Some("{}"),
            None,
            None,
        )
        .await
        .expect("enqueue");
    let run_id = format!("brain-job-{job_id}");

    assert!(db
        .task_control(&run_id, 1, screenpipe_db::TaskControl::Cancel)
        .await
        .expect("cancel queued run"));
    assert_eq!(
        db.knowledge_get_job(job_id).await.unwrap().unwrap().state,
        "cancelled"
    );
    let cancelled = db.task_get_run(&run_id).await.unwrap().unwrap();
    assert_eq!(cancelled.state, TaskState::Cancelled);

    assert!(db
        .task_control(
            &run_id,
            cancelled.revision,
            screenpipe_db::TaskControl::Retry
        )
        .await
        .expect("retry cancelled run"));
    assert_eq!(
        db.knowledge_get_job(job_id).await.unwrap().unwrap().state,
        "pending"
    );
    assert_eq!(
        db.task_get_run(&run_id).await.unwrap().unwrap().state,
        TaskState::Queued
    );

    let queued = db.task_get_run(&run_id).await.unwrap().unwrap();
    assert!(db
        .task_control(&run_id, queued.revision, screenpipe_db::TaskControl::Pause)
        .await
        .expect("pause queued run"));
    assert_eq!(
        db.knowledge_get_job(job_id).await.unwrap().unwrap().state,
        "paused"
    );
    let paused = db.task_get_run(&run_id).await.unwrap().unwrap();
    assert!(db
        .task_control(&run_id, paused.revision, screenpipe_db::TaskControl::Resume)
        .await
        .expect("resume paused run"));
    assert_eq!(
        db.knowledge_get_job(job_id).await.unwrap().unwrap().state,
        "pending"
    );
    assert_eq!(
        db.task_get_run(&run_id).await.unwrap().unwrap().state,
        TaskState::Queued
    );
}
