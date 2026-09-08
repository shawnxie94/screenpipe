// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

use std::sync::Arc;

use screenpipe_db::{DatabaseManager, TaskKind, TaskResourceClass, TaskRunRequest};
use screenpipe_engine::tasks::{user_pipe_definition_id, TaskService, UserPipeTaskSpec};
use serde_json::json;

async fn db() -> Arc<DatabaseManager> {
    Arc::new(
        DatabaseManager::new("sqlite::memory:", Default::default())
            .await
            .unwrap(),
    )
}

#[tokio::test]
async fn lifecycle_compatibility_service_registers_builtin_and_uses_the_shared_run_store() {
    let db = db().await;
    let service = TaskService::new(db.clone());
    assert!(service
        .register_builtin(
            "activity-summary",
            TaskKind::ActivitySummary,
            TaskResourceClass::Extract
        )
        .await
        .unwrap());
    assert_eq!(db.task_list_definitions().await.unwrap().len(), 1);
    let (run_id, created) = db
        .task_start_run(&TaskRunRequest {
            definition_id: Some("activity-summary".to_string()),
            definition_revision: "builtin-v1".to_string(),
            root_run_id: None,
            parent_run_id: None,
            retry_of: None,
            trigger_key: "test".to_string(),
            input_hash: "activity-input".to_string(),
            input_refs: json!({"coverage":"synthetic"}),
            config_snapshot: json!({}),
            priority: 100,
            not_before: None,
            deadline: None,
            owner_generation: 0,
        })
        .await
        .unwrap();
    assert!(created);
    assert_eq!(
        db.task_get_run(&run_id).await.unwrap().unwrap().root_run_id,
        run_id
    );
}

#[tokio::test]
async fn user_pipe_projection_is_stable_and_source_bound() {
    let db = db().await;
    let service = TaskService::new(db.clone());
    let spec = UserPipeTaskSpec {
        name: "daily-pipe".into(),
        config_revision: "pipe-hash-1".into(),
        enabled: true,
        trigger: screenpipe_db::TaskTrigger {
            kind: "interval".into(),
            expression: Some("every 1h".into()),
        },
        model_binding_policy: Some("selected-preset".into()),
    };

    service
        .sync_user_pipe_definitions(std::slice::from_ref(&spec))
        .await
        .unwrap();
    let id = user_pipe_definition_id("daily-pipe");
    let first = db.task_get_definition(&id).await.unwrap().unwrap();
    assert_eq!(first.origin, screenpipe_db::TaskOrigin::User);
    assert_eq!(first.kind, TaskKind::PipeRun);
    assert_eq!(first.config_ref.as_deref(), Some("pipe:daily-pipe"));
    assert_eq!(first.migration_state, "unified_projection");

    service
        .sync_user_pipe_definitions(std::slice::from_ref(&spec))
        .await
        .unwrap();
    let second = db.task_get_definition(&id).await.unwrap().unwrap();
    assert_eq!(second.revision, first.revision);
    assert_eq!(second.config_revision, first.config_revision);

    let removed = UserPipeTaskSpec {
        name: "removed-pipe".into(),
        ..spec.clone()
    };
    service
        .sync_user_pipe_definitions(&[spec.clone(), removed])
        .await
        .unwrap();
    service
        .sync_user_pipe_definitions(std::slice::from_ref(&spec))
        .await
        .unwrap();
    let retired = db
        .task_get_definition(&user_pipe_definition_id("removed-pipe"))
        .await
        .unwrap()
        .unwrap();
    assert!(!retired.enabled);
    assert_eq!(retired.migration_state, "retired");
}
