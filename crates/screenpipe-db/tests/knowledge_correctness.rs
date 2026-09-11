// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
// Review-derived Local Knowledge persistence probes. Synthetic SQLite only.

use std::sync::Arc;

use chrono::{Duration, Utc};
use screenpipe_db::{
    compute_input_hash, KnowledgeJobKind, KnowledgeSearchDocInput, KnowledgeSourceInput, DatabaseManager,
    DeletionCause, SourceKind, SourceLocator,
};

fn frame_input_at(
    table: &str,
    id: i64,
    revision: &str,
    captured_at: chrono::DateTime<Utc>,
) -> KnowledgeSourceInput {
    KnowledgeSourceInput {
        kind: SourceKind::Frame,
        locator: SourceLocator::new(table, id),
        revision: revision.to_string(),
        fingerprint_inputs: serde_json::json!({"text": revision}),
        captured_at,
        app: Some("Safari".into()),
        window_name: None,
        evidence_method: "ocr".into(),
        media_available: true,
        office: None,
        excerpt: Some(format!("text {revision}")),
    }
}

fn frame_input(table: &str, id: i64, revision: &str) -> KnowledgeSourceInput {
    frame_input_at(table, id, revision, Utc::now())
}

/// A source fixture must point at an actual capture row, otherwise a future
/// source existence check could make a negative test pass for the wrong reason.
async fn real_frame_input(db: &Arc<DatabaseManager>, revision: &str) -> KnowledgeSourceInput {
    let captured_at = Utc::now();
    let mut tx = db.begin_immediate_with_retry().await.unwrap();
    let frame_id = sqlx::query(
        "INSERT INTO frames (timestamp, app_name, window_name, focused, device_name, full_text, text_source) \
         VALUES (?1, ?2, ?3, 1, ?4, ?5, 'ocr')",
    )
    .bind(captured_at)
    .bind("Safari")
    .bind("fixture-window")
    .bind("f01-db-device")
    .bind(format!("text {revision}"))
    .execute(&mut **tx.conn())
    .await
    .unwrap()
    .last_insert_rowid();
    tx.commit().await.unwrap();
    frame_input_at("frames", frame_id, revision, captured_at)
}

async fn db() -> Arc<DatabaseManager> {
    Arc::new(
        DatabaseManager::new("sqlite::memory:", Default::default())
            .await
            .unwrap(),
    )
}

#[tokio::test]
async fn migration_db_source_registration_is_idempotent_and_revision_aware() {
    let db = db().await;
    let first = db
        .knowledge_register_source(&frame_input("frames", 1, "rev-a"))
        .await
        .unwrap();
    assert!(first.created && !first.suppressed);

    let same = db
        .knowledge_register_source(&frame_input("frames", 1, "rev-a"))
        .await
        .unwrap();
    assert_eq!(same.source_uid, first.source_uid);
    assert!(!same.created && !same.revision_changed);

    let changed = db
        .knowledge_register_source(&frame_input("frames", 1, "rev-b"))
        .await
        .unwrap();
    assert_eq!(changed.source_uid, first.source_uid);
    assert!(changed.revision_changed);
    assert_eq!(
        db.knowledge_get_source(&first.source_uid)
            .await
            .unwrap()
            .unwrap()
            .revision,
        "rev-b"
    );
}

#[tokio::test]
async fn search_compatibility_projection_upsert_query_and_delete() {
    let db = db().await;
    let input = KnowledgeSearchDocInput {
        doc_id: "source:compat-1".into(),
        kind: "source".into(),
        ref_uid: "compat-1".into(),
        ref_revision: "rev-a".into(),
        title: Some("Compatibility fixture".into()),
        body: "compatmarker first revision".into(),
        app: Some("fixture-app".into()),
        event_at: Some("2026-09-08T09:00:00Z".into()),
    };
    db.knowledge_search_upsert_doc(&input).await.unwrap();

    let first = db
        .knowledge_search_topk("compatmarker", &["source"], 10)
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].ref_revision, "rev-a");
    assert_eq!(first[0].body, input.body);
    assert!(db
        .knowledge_search_topk("compatmarker", &["knowledge"], 10)
        .await
        .unwrap()
        .is_empty());

    let updated = KnowledgeSearchDocInput {
        ref_revision: "rev-b".into(),
        body: "compatmarker updated revision".into(),
        ..input.clone()
    };
    db.knowledge_search_upsert_doc(&updated).await.unwrap();
    let second = db
        .knowledge_search_topk("compatmarker", &["source"], 10)
        .await
        .unwrap();
    assert_eq!(second.len(), 1, "upsert must not duplicate the projection");
    assert_eq!(second[0].ref_revision, "rev-b");
    assert_eq!(second[0].body, updated.body);

    db.knowledge_search_delete_doc(&updated.doc_id).await.unwrap();
    assert!(db
        .knowledge_search_topk("compatmarker", &["source"], 10)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn deletion_db_user_erase_tombstones_source_and_blocks_reimport() {
    let db = db().await;
    let registration = db
        .knowledge_register_source(&frame_input("frames", 7, "rev-a"))
        .await
        .unwrap();

    db.knowledge_delete_source(&registration.source_uid, true)
        .await
        .unwrap();
    assert!(db
        .knowledge_get_source(&registration.source_uid)
        .await
        .unwrap()
        .is_none());

    let reimport = db
        .knowledge_register_source(&frame_input("frames", 7, "rev-a"))
        .await
        .unwrap();
    assert!(reimport.suppressed, "deleted content must not re-register");
}

#[tokio::test]
async fn jobs_db_job_lifecycle_deduplicates_claims_and_guards_tokens() {
    let db = db().await;
    let hash = compute_input_hash(
        &[("s1".into(), "r1".into())],
        "scope",
        "classification-v1",
        "extract-v1",
        "prompt-v1",
        "skill-v1",
    );
    let (first_id, first_created) = db
        .knowledge_enqueue_job(
            KnowledgeJobKind::Extract,
            Some("scope"),
            Some(&hash),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(first_created);
    let (same_id, same_created) = db
        .knowledge_enqueue_job(
            KnowledgeJobKind::Extract,
            Some("scope"),
            Some(&hash),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(same_id, first_id);
    assert!(!same_created, "active duplicate input must dedupe");

    let claimed = db
        .knowledge_claim_next_job(&[KnowledgeJobKind::Extract], "worker-a", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, first_id);
    assert!(db
        .knowledge_claim_next_job(&[KnowledgeJobKind::Extract], "worker-b", 30_000)
        .await
        .unwrap()
        .is_none());
    assert!(!db
        .knowledge_complete_job(first_id, "stale-token", None, None)
        .await
        .unwrap());
    assert!(db
        .knowledge_complete_job(
            first_id,
            &claimed.lease_token,
            Some("result"),
            Some("cursor")
        )
        .await
        .unwrap());
    assert_eq!(
        db.knowledge_get_job(first_id).await.unwrap().unwrap().state,
        "succeeded"
    );
}

#[tokio::test]
async fn jobs_db_expired_lease_reaps_while_preserving_model_call_count() {
    let db = db().await;
    let (job_id, _) = db
        .knowledge_enqueue_job(KnowledgeJobKind::Answer, None, None, None, None, None)
        .await
        .unwrap();
    let claimed = db
        .knowledge_claim_next_job(&[KnowledgeJobKind::Answer], "worker", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert!(db
        .knowledge_job_model_call(job_id, &claimed.lease_token)
        .await
        .unwrap());
    let mut tx = db.begin_immediate_with_retry().await.unwrap();
    sqlx::query("UPDATE knowledge_jobs SET lease_expires_at = '2000-01-01T00:00:00Z' WHERE id = ?")
        .bind(job_id)
        .execute(&mut **tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(db.knowledge_reap_expired_leases().await.unwrap(), 1);
    let row = db.knowledge_get_job(job_id).await.unwrap().unwrap();
    assert_eq!(row.state, "pending");
    assert_eq!(row.model_calls, 1, "call counters never reset");
    assert!(!db
        .knowledge_complete_job(job_id, &claimed.lease_token, None, None)
        .await
        .unwrap());
}

#[tokio::test]
async fn deletion_db_deletion_barrier_cancels_jobs_and_invalidates_consumers() {
    let db = db().await;
    let registration = db
        .knowledge_register_source(&real_frame_input(&db, "rev-a").await)
        .await
        .unwrap();
    let work_unit = db
        .knowledge_save_work_unit(
            "deletion-scope",
            Some("deletion-task"),
            Some("2026-09-08T00:00:00Z"),
            Some("2026-09-08T00:01:00Z"),
            "deletion-input",
            "extractor-v1",
            "prompt-v1",
            "deletion fixture body",
        )
        .await
        .unwrap();
    db.knowledge_register_dependency(
        "work_unit",
        &work_unit,
        None,
        None,
        &registration.source_uid,
        Some("rev-a"),
    )
    .await
    .unwrap();
    let knowledge = db
        .knowledge_create_knowledge_candidate(
            "sop",
            "deletion-scope",
            "deletion fixture",
            r#"{"body":"deletion fixture knowledge"}"#,
            "deletion-knowledge-input",
            std::slice::from_ref(&work_unit),
        )
        .await
        .unwrap()
        .0;
    db.knowledge_register_dependency(
        "knowledge",
        &knowledge,
        Some("1"),
        None,
        &registration.source_uid,
        Some("rev-a"),
    )
    .await
    .unwrap();
    let (job_id, _) = db
        .knowledge_enqueue_job(
            KnowledgeJobKind::Extract,
            Some(&work_unit),
            Some("ih-1"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let deletion_id = db
        .knowledge_record_deletion(
            DeletionCause::UserErase,
            &format!(
                r#"{{"kind":"sources","uids":["{}"]}}"#,
                registration.source_uid
            ),
        )
        .await
        .unwrap();
    db.knowledge_activate_deletion_barrier(deletion_id, true)
        .await
        .unwrap();
    assert_eq!(
        db.knowledge_get_job(job_id).await.unwrap().unwrap().state,
        "cancelled"
    );
    let consumers = db
        .knowledge_invalidate_consumers(&[registration.source_uid])
        .await
        .unwrap();
    assert!(consumers
        .iter()
        .any(|hit| hit.consumer_kind == "knowledge" && hit.consumer_id == knowledge));
    // Low-level characterization: this DB method invalidates work-unit state
    // and returns knowledge hits. Engine P01 owns end-to-end erase/body
    // clearing; this test must not treat retained low-level body as allowed.
    let work_unit_state: String =
        sqlx::query_scalar("SELECT state FROM knowledge_work_units WHERE id = ?1")
            .bind(&work_unit)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(work_unit_state, "invalidated");
    let revision_body: String =
        sqlx::query_scalar("SELECT body FROM knowledge_work_unit_revisions WHERE work_unit_id = ?1")
            .bind(&work_unit)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(revision_body, "deletion fixture body");
    let knowledge_row = db.knowledge_get_version(&knowledge, 1).await.unwrap().unwrap();
    assert_eq!(
        knowledge_row.body,
        r#"{"body":"deletion fixture knowledge"}"#
    );
}

#[tokio::test]
async fn publication_db_retention_preserves_published_excerpt_before_raw_delete() {
    let db = db().await;
    let captured = Utc::now() - Duration::hours(2);
    let registration = db
        .knowledge_register_source(&real_frame_input(&db, "retention-rev").await)
        .await
        .unwrap();
    let frame_id = db
        .knowledge_get_source(&registration.source_uid)
        .await
        .unwrap()
        .unwrap()
        .locator_id;
    // Move the source timestamp into the tested retention range while keeping
    // the locator pointed at the real frame fixture.
    let mut tx = db.begin_immediate_with_retry().await.unwrap();
    sqlx::query("UPDATE knowledge_sources SET captured_at = ?1 WHERE source_uid = ?2")
        .bind(captured)
        .bind(&registration.source_uid)
        .execute(&mut **tx.conn())
        .await
        .unwrap();
    sqlx::query("UPDATE frames SET timestamp = ?1 WHERE id = ?2")
        .bind(captured)
        .bind(frame_id)
        .execute(&mut **tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let knowledge = db
        .knowledge_create_knowledge_candidate(
            "decision_rule",
            "retention-scope",
            "retention fixture",
            r#"{"schema_version":1,"title":"retention fixture","condition":"observed","rule":"keep","evidence_refs":["u1"]}"#,
            "retention-input",
            &[],
        )
        .await
        .unwrap()
        .0;
    db.knowledge_register_dependency(
        "knowledge",
        &knowledge,
        Some("1"),
        None,
        &registration.source_uid,
        Some("retention-rev"),
    )
    .await
    .unwrap();
    let version = db.knowledge_get_version(&knowledge, 1).await.unwrap().unwrap();
    assert!(db
        .knowledge_publish_version(
            &knowledge,
            1,
            &version.revision,
            None,
            "2099-01-01T00:00:00Z"
        )
        .await
        .unwrap());
    let matched: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM knowledge_sources s JOIN knowledge_dependencies d ON d.source_uid = s.source_uid JOIN knowledge_item_versions v ON v.knowledge_id = d.consumer_id WHERE s.source_uid = ?1 AND s.captured_at >= ?2 AND s.captured_at < ?3 AND d.consumer_kind = 'knowledge' AND v.state = 'published' AND v.availability = 'valid'",
    )
    .bind(&registration.source_uid)
    .bind(captured - Duration::minutes(1))
    .bind(captured + Duration::minutes(1))
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        matched, 1,
        "published source must match retention protection predicate"
    );
    let result = db
        .delete_time_range_batch(
            captured - Duration::minutes(1),
            captured + Duration::minutes(1),
            true,
        )
        .await
        .unwrap();
    assert_eq!(result.frames_deleted, 1);
    let source = db
        .knowledge_get_source(&registration.source_uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(source.state, "archived");
    let text = db
        .knowledge_source_text(&registration.source_uid, "retention-rev")
        .await
        .unwrap()
        .expect("published source excerpt must remain readable");
    assert_eq!(text.0, "text retention-rev");
    assert!(text.1);
}

#[tokio::test]
async fn publication_db_publish_lifts_feedback_pause_so_the_correction_loop_recloses() {
    let db = db().await;
    let knowledge = db
        .knowledge_create_knowledge_candidate(
            "decision_rule",
            "pause-reclose-scope",
            "pause reclose fixture",
            r#"{"schema_version":1,"title":"pause reclose fixture","condition":"observed","rule":"keep","evidence_refs":["u1"]}"#,
            "pause-reclose-input",
            &[],
        )
        .await
        .unwrap()
        .0;
    let version = db.knowledge_get_version(&knowledge, 1).await.unwrap().unwrap();
    assert!(db
        .knowledge_publish_version(&knowledge, 1, &version.revision, None, "2099-01-01T00:00:00Z")
        .await
        .unwrap());

    // Incorrect feedback suspends the whole knowledge row (locator pause),
    // which removes it from retrieval until a corrected publish lands.
    db.knowledge_set_paused(&knowledge, true).await.unwrap();
    let paused: i64 = sqlx::query_scalar("SELECT paused FROM knowledge_items WHERE id = ?1")
        .bind(&knowledge)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(paused, 1, "feedback pause must hold before the corrected publish");

    let candidate = db
        .knowledge_add_knowledge_candidate_version(
            &knowledge,
            "pause reclose fixture v2",
            &version.body,
            "pause-reclose-input-v2",
            &[],
        )
        .await
        .unwrap();
    let candidate_row = db.knowledge_get_version(&knowledge, candidate).await.unwrap().unwrap();
    assert!(db
        .knowledge_publish_version(
            &knowledge,
            candidate,
            &candidate_row.revision,
            Some(1),
            "2099-01-01T00:00:00Z"
        )
        .await
        .unwrap());

    let (current, paused_after): (Option<i64>, i64) = sqlx::query_as(
        "SELECT current_version_id, paused FROM knowledge_items WHERE id = ?1",
    )
    .bind(&knowledge)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(current, Some(candidate), "v2 must become the current version");
    assert_eq!(
        paused_after, 0,
        "publishing the corrected version must lift the feedback pause"
    );
}
