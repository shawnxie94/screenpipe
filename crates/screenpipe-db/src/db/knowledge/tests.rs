// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

use super::types::{
    compute_input_hash, DeletionCause, KnowledgeAvailability, KnowledgeState, SourceKind,
};
use super::*;

use chrono::Utc;

fn frame_input(table: &str, id: i64, revision: &str) -> super::sources::KnowledgeSourceInput {
    super::sources::KnowledgeSourceInput {
        kind: SourceKind::Frame,
        locator: super::types::SourceLocator::new(table, id),
        revision: revision.to_string(),
        fingerprint_inputs: serde_json::json!({"text": revision}),
        captured_at: Utc::now(),
        app: Some("Safari".into()),
        window_name: None,
        evidence_method: "ocr".into(),
        media_available: true,
        office: None,
        excerpt: Some(format!("text {revision}")),
    }
}

#[tokio::test]
async fn register_source_is_idempotent_and_revision_aware() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let first = db
        .knowledge_register_source(&frame_input("frames", 1, "rev-a"))
        .await
        .unwrap();
    assert!(first.created && !first.suppressed);

    // Same content again: same identity, no new revision.
    let again = db
        .knowledge_register_source(&frame_input("frames", 1, "rev-a"))
        .await
        .unwrap();
    assert_eq!(again.source_uid, first.source_uid);
    assert!(!again.created && !again.revision_changed);

    // Changed content: same identity, new revision.
    let changed = db
        .knowledge_register_source(&frame_input("frames", 1, "rev-b"))
        .await
        .unwrap();
    assert_eq!(changed.source_uid, first.source_uid);
    assert!(changed.revision_changed);

    let row = db
        .knowledge_get_source(&first.source_uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.revision, "rev-b");
    assert!(db.knowledge_current_deletion_epoch().await.unwrap() >= 0);
}

#[tokio::test]
async fn user_deleted_source_is_tombstoned_and_never_reimports() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let reg = db
        .knowledge_register_source(&frame_input("frames", 7, "rev-a"))
        .await
        .unwrap();

    db.knowledge_delete_source(&reg.source_uid, true).await.unwrap();
    assert!(db
        .knowledge_get_source(&reg.source_uid)
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
async fn office_identity_is_provider_scoped() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let office = super::types::OfficeSourceMeta {
        provider: "feishu".into(),
        account_namespace: "acc-1".into(),
        object_id: "om_1".into(),
        object_revision: None,
        event_at: Some(Utc::now()),
        fetched_at: Utc::now(),
        source_url: Some("https://example.feishu.cn/m/om_1".into()),
        completeness: "full".into(),
        activity_anchor: Some("chat:oc_1".into()),
        platform_generated: false,
    };
    let input = super::sources::KnowledgeSourceInput {
        kind: SourceKind::OfficeMessage,
        locator: super::types::SourceLocator::new("knowledge_office_objects", 0),
        revision: "hash-1".into(),
        fingerprint_inputs: serde_json::json!({}),
        captured_at: Utc::now(),
        app: None,
        window_name: None,
        evidence_method: "office_import".into(),
        media_available: true,
        office: Some(office.clone()),
        excerpt: Some("消息正文".into()),
    };
    let a = db.knowledge_register_source(&input).await.unwrap();
    assert!(a.created);
    // Same object under a different account is a different identity.
    let mut other_acc = input.clone();
    if let Some(o) = other_acc.office.as_mut() {
        o.account_namespace = "acc-2".into();
    }
    let b = db.knowledge_register_source(&other_acc).await.unwrap();
    assert!(b.created);
    assert_ne!(a.source_uid, b.source_uid);
    // Same object again dedupes.
    let c = db.knowledge_register_source(&input).await.unwrap();
    assert_eq!(c.source_uid, a.source_uid);
    assert!(!c.created);
}

#[tokio::test]
async fn deletion_barrier_cancels_jobs_and_propagates_consumers() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let reg = db
        .knowledge_register_source(&frame_input("frames", 3, "rev-a"))
        .await
        .unwrap();

    // Work unit revision + knowledge depending on the source.
    db.knowledge_register_dependency(
        "work_unit",
        "wu-1",
        None,
        None,
        &reg.source_uid,
        Some("rev-a"),
    )
    .await
    .unwrap();
    db.knowledge_register_dependency(
        "work_unit",
        "wu-1",
        Some("ih-1"),
        None,
        &reg.source_uid,
        Some("rev-a"),
    )
    .await
    .unwrap();
    db.knowledge_register_dependency(
        "knowledge",
        "kn-1",
        Some("2"),
        None,
        &reg.source_uid,
        Some("rev-a"),
    )
    .await
    .unwrap();

    let (job_id, created) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("wu-1"),
            Some("ih-1"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(created);

    let deletion_id = db
        .knowledge_record_deletion(
            DeletionCause::UserErase,
            r#"{"kind":"sources","uids":["x"]}"#,
        )
        .await
        .unwrap();
    db.knowledge_activate_deletion_barrier(deletion_id, true)
        .await
        .unwrap();

    let job = db.knowledge_get_job(job_id).await.unwrap().unwrap();
    assert_eq!(
        job.state, "cancelled",
        "active jobs must not survive a deletion barrier"
    );

    sqlx::query("INSERT INTO knowledge_work_units (id, scope_key) VALUES ('wu-1', 'scope-a')")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO knowledge_work_unit_revisions (work_unit_id, input_hash, extractor_schema_version, prompt_version, body) \
         VALUES ('wu-1', 'ih-1', 'v1', 'p1', '{}')",
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let hits = db
        .knowledge_invalidate_consumers(&[reg.source_uid.clone()])
        .await
        .unwrap();
    assert!(hits
        .iter()
        .any(|h| h.consumer_kind == "knowledge" && h.consumer_id == "kn-1"));

    let wu_rev: (String,) =
        sqlx::query_as("SELECT state FROM knowledge_work_unit_revisions WHERE work_unit_id = 'wu-1'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(wu_rev.0, "invalidated");
}

#[tokio::test]
async fn archived_excerpt_survives_retention_but_user_erase_wins() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let reg = db
        .knowledge_register_source(&frame_input("frames", 11, "rev-a"))
        .await
        .unwrap();
    db.knowledge_ensure_archived_excerpt(&reg.source_uid, "rev-a", "必要原文节选")
        .await
        .unwrap();
    let text = db
        .knowledge_source_text(&reg.source_uid, "rev-a")
        .await
        .unwrap();
    assert_eq!(text, Some(("必要原文节选".to_string(), true)));

    // Retention-style removal (no tombstone): archived excerpt row goes with
    // the source, matching the conservative whole-body rule.
    db.knowledge_delete_source(&reg.source_uid, false)
        .await
        .unwrap();
    assert!(db
        .knowledge_source_text(&reg.source_uid, "rev-a")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn job_lifecycle_dedupe_claim_and_token_guard() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let hash = compute_input_hash(&[("s1".into(), "r1".into())], "scope", "c", "v1", "p1", "skill-v1");
    let (id1, created1) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("s"),
            Some(&hash),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(created1);
    let (id2, created2) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("s"),
            Some(&hash),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(
        !created2 && id1 == id2,
        "active duplicate input must dedupe"
    );

    let claimed = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Extract], "worker-a", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, id1);

    // A second worker cannot claim the same running job.
    let second = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Extract], "worker-b", 30_000)
        .await
        .unwrap();
    assert!(second.is_none());

    // Stale tokens cannot commit.
    assert!(!db
        .knowledge_complete_job(claimed.id, "bogus-token", None, None)
        .await
        .unwrap());
    assert!(db
        .knowledge_complete_job(
            claimed.id,
            &claimed.lease_token,
            Some("res"),
            Some("cursor-1")
        )
        .await
        .unwrap());
    let done = db.knowledge_get_job(id1).await.unwrap().unwrap();
    assert_eq!(done.state, "succeeded");
    assert_eq!(done.batch_id, None);
}

/// §4.1.1 counterexample to the old retry policy: the FIRST failure — even
/// one classified transient — is terminal. Nothing may auto-claim the job
/// again; the public run lands in `failed` in the same transaction, and only
/// the explicit manual retry reopens it (exactly once per invocation), with
/// the recorded error still traceable.
#[tokio::test]
async fn first_failure_is_terminal_until_manual_retry() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let (id, _) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Compile,
            Some("s"),
            Some("h"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let claimed = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Compile], "w", 30_000)
        .await
        .unwrap()
        .unwrap();
    // A transient-classified failure must still be terminal.
    assert!(db
        .knowledge_fail_job(claimed.id, &claimed.lease_token, "model_timeout", Some("模型调用超时"), true, 1_000)
        .await
        .unwrap());
    let job = db.knowledge_get_job(id).await.unwrap().unwrap();
    assert_eq!(job.state, "failed", "transient classification must not requeue");
    assert_eq!(job.attempts, 1);
    assert_eq!(job.last_error_code.as_deref(), Some("model_timeout"));
    assert_eq!(job.last_error_message.as_deref(), Some("模型调用超时"));
    // No automatic re-claim: the queue stays empty.
    assert!(db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Compile], "w", 30_000)
        .await
        .unwrap()
        .is_none());
    // The public run is failed in the same transaction.
    let run_state: String =
        sqlx::query_scalar("SELECT state FROM task_runs WHERE run_id = ?1")
            .bind(format!("brain-job-{id}"))
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(run_state, "failed");

    // The explicit manual retry is the only way back — and it reopens once.
    assert!(db.knowledge_retry_job(id).await.unwrap());
    let reopened = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Compile], "w", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reopened.id, id);
    // A second manual retry while the job is live must not duplicate it.
    assert!(!db.knowledge_retry_job(id).await.unwrap());
    // History stays traceable across the manual retry.
    let job = db.knowledge_get_job(id).await.unwrap().unwrap();
    assert_eq!(job.last_error_code.as_deref(), Some("model_timeout"));
    assert_eq!(job.last_error_message.as_deref(), Some("模型调用超时"));
}

/// The office-sync owner keeps its own retry policy (attempt 3): transient
/// failures requeue with backoff up to max_attempts — finite, not an infinite
/// loop — and a permanent failure is still terminal.
#[tokio::test]
async fn office_sync_owner_keeps_its_backoff_policy() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let (id, _) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::OfficeSync,
            Some("feishu:acc-1"),
            Some("oh-1"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    for round in 1..=3 {
        let claimed = db
            .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::OfficeSync], "w", 30_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id, id, "round {round}: claimable again after backoff");
        assert!(db
            .knowledge_fail_job(claimed.id, &claimed.lease_token, "transcript_pending", None, true, 15 * 60 * 1_000)
            .await
            .unwrap());
        let job = db.knowledge_get_job(id).await.unwrap().unwrap();
        if round < 3 {
            assert_eq!(job.state, "pending", "office sync backoff is unaffected");
        } else {
            assert_eq!(job.state, "failed", "round 3 exhausts max_attempts");
        }
        assert_eq!(job.attempts, round as i32);
        // Expire the backoff so the next round can claim immediately.
        sqlx::query("UPDATE knowledge_jobs SET not_before = '2000-01-01T00:00:00Z' WHERE id = ?1")
            .bind(id)
            .execute(&db.pool)
            .await
            .unwrap();
    }
    // max_attempts (3) exhausted: the owner's own policy terminates too — no
    // infinite automatic retry.
    let job = db.knowledge_get_job(id).await.unwrap().unwrap();
    assert_eq!(job.state, "failed", "transient backoff must stay finite");
    assert!(db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::OfficeSync], "w", 30_000)
        .await
        .unwrap()
        .is_none());
}

/// Lease recovery follows the owner mapping (attempt 3): knowledge model
/// kinds fail terminally in BOTH stores, while non-knowledge owners keep the
/// original requeue-on-expiry pair (job pending + run queued, tokens cleared)
/// so the stores never diverge — regardless of which reaper runs first.
#[tokio::test]
async fn expired_lease_recovery_follows_owner_policy() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let (model_job, _) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-m"),
            Some("hash-m"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let (office_job, _) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::OfficeSync,
            Some("feishu:acc-1"),
            Some("hash-o"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let model_claim = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Extract], "w", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(model_claim.id, model_job);
    let office_claim = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::OfficeSync], "w", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(office_claim.id, office_job);
    for job_id in [model_job, office_job] {
        sqlx::query("UPDATE knowledge_jobs SET lease_expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1")
            .bind(job_id)
            .execute(&db.pool)
            .await
            .unwrap();
    }

    assert_eq!(db.knowledge_reap_expired_leases().await.unwrap(), 2);

    // Model kind: terminal in both stores; nothing re-claims it.
    let model = db.knowledge_get_job(model_job).await.unwrap().unwrap();
    assert_eq!(model.state, "failed");
    assert_eq!(model.last_error_code.as_deref(), Some("lease_expired"));
    let model_run: String =
        sqlx::query_scalar("SELECT state FROM task_runs WHERE run_id = ?1")
            .bind(format!("brain-job-{model_job}"))
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(model_run, "failed");

    // Office owner: legacy requeue pair — job pending, run queued, tokens
    // cleared so the next claim rebinds cleanly.
    let office = db.knowledge_get_job(office_job).await.unwrap().unwrap();
    assert_eq!(office.state, "pending", "office lease recovery requeues");
    let office_token: Option<String> =
        sqlx::query_scalar("SELECT lease_token FROM knowledge_jobs WHERE id = ?1")
            .bind(office_job)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(office_token.is_none(), "stale token cleared");
    let office_run: String =
        sqlx::query_scalar("SELECT state FROM task_runs WHERE run_id = ?1")
            .bind(format!("brain-job-{office_job}"))
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(office_run, "queued");
    // Consistent on a second pass: nothing left to reap.
    assert_eq!(db.knowledge_reap_expired_leases().await.unwrap(), 0);
}

#[tokio::test]
async fn expired_lease_fails_terminally_with_counters_preserved() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let (id, _) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-l"),
            Some("hash-l"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let claimed = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Extract], "w", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, id);
    assert!(db
        .knowledge_job_model_call(id, &claimed.lease_token)
        .await
        .unwrap());
    // Simulate a dead worker: lease in the past.
    sqlx::query("UPDATE knowledge_jobs SET lease_expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1")
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    let reaped = db.knowledge_reap_expired_leases().await.unwrap();
    assert_eq!(reaped, 1);
    let job = db.knowledge_get_job(id).await.unwrap().unwrap();
    assert_eq!(job.state, "failed", "a lost lease must not auto re-run the model");
    assert_eq!(job.last_error_code.as_deref(), Some("lease_expired"));
    assert_eq!(job.model_calls, 1, "call counters never reset");
    // The dead worker's stale token cannot commit after the reap.
    assert!(!db
        .knowledge_complete_job(id, &claimed.lease_token, None, None)
        .await
        .unwrap());
    // Nothing is claimable: no unauthorized second model run.
    assert!(db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Extract], "w", 30_000)
        .await
        .unwrap()
        .is_none());
    // The public run is failed too.
    let run_state: String =
        sqlx::query_scalar("SELECT state FROM task_runs WHERE run_id = ?1")
            .bind(format!("brain-job-{id}"))
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(run_state, "failed");
}

/// Terminal cleanup must not silently revive failed inputs (§4.1.1): the
/// pruner only retires `succeeded`/`cancelled` rows, so a failed row keeps
/// blocking re-enqueue of the same inputs until a human retries it.
#[tokio::test]
async fn terminal_prune_never_revives_failed_inputs() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    // A failed job...
    let (failed_id, _) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-f"),
            Some("hash-fail"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let claimed = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Extract], "w", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, failed_id);
    assert!(db
        .knowledge_fail_job(claimed.id, &claimed.lease_token, "no_evidence", Some("无证据"), false, 0)
        .await
        .unwrap());
    // ...and a succeeded one.
    let (succeeded_id, _) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-s"),
            Some("hash-ok"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let claimed = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Extract], "w", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, succeeded_id);
    assert!(db
        .knowledge_complete_job(claimed.id, &claimed.lease_token, None, None)
        .await
        .unwrap());
    // Age the succeeded row beyond the pruner's keep window (newest-N by
    // updated_at) and fill the window with 10 newer succeeded rows, so the
    // prune actually retires the old one.
    sqlx::query("UPDATE knowledge_jobs SET updated_at = '2000-01-01T00:00:00Z' WHERE id = ?1")
        .bind(succeeded_id)
        .execute(&db.pool)
        .await
        .unwrap();
    for k in 0..10 {
        let (filler, _) = db
            .knowledge_enqueue_job(
                super::types::KnowledgeJobKind::Extract,
                Some(&format!("scope-fill-{k}")),
                Some(&format!("hash-fill-{k}")),
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let claimed = db
            .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Extract], "w", 30_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id, filler);
        assert!(db
            .knowledge_complete_job(claimed.id, &claimed.lease_token, None, None)
            .await
            .unwrap());
    }

    db.knowledge_prune_terminal_jobs(10).await.unwrap();
    let failed_gone: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM knowledge_jobs WHERE id = ?1",
    )
    .bind(failed_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(failed_gone, 1, "failed rows survive cleanup as dedup anchors");
    let succeeded_gone: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM knowledge_jobs WHERE id = ?1",
    )
    .bind(succeeded_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(succeeded_gone, 0, "succeeded rows are prunable");

    // The failed input is still blocked after the cleanup round.
    let (_, created) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-f"),
            Some("hash-fail"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(!created, "cleanup must not open a path back for failed inputs");
}

#[tokio::test]
async fn knowledge_state_enums_roundtrip() {
    assert_eq!(
        KnowledgeState::from_str(KnowledgeState::Candidate.as_str()),
        Some(KnowledgeState::Candidate)
    );
    assert_eq!(
        KnowledgeAvailability::from_str(KnowledgeAvailability::ReviewDue.as_str()),
        Some(KnowledgeAvailability::ReviewDue)
    );
}

/// Failure is a terminal state (plan §4.1.1): a succeeded or failed job with
/// the same (kind, input_hash) blocks re-enqueueing, so failed work is never
/// rebuilt by the next discovery scan — a changed input hash is the only
/// automatic path to a new job, and manual retry stays available. The scope
/// needs no separate binding: the table's partial unique index
/// `idx_knowledge_jobs_active_input` pins the same identity, and every hash
/// family embeds its scope.
#[tokio::test]
async fn terminal_jobs_block_reenqueue_until_the_input_hash_changes() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();

    // Succeeded terminal state blocks the identical input.
    let (succeeded_id, _) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-a"),
            Some("hash-1"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let claimed = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Extract], "w", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert!(db
        .knowledge_complete_job(claimed.id, &claimed.lease_token, None, None)
        .await
        .unwrap());
    let (again_id, again_created) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-a"),
            Some("hash-1"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(!again_created, "a succeeded job must block re-enqueueing");
    assert_eq!(again_id, succeeded_id);

    // Changed input hash → new job (this is the only automatic re-entry).
    let (_, changed_created) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-a"),
            Some("hash-2"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(changed_created, "a changed input hash must allow a new job");
    // Drain it so later claims in this test see an empty queue.
    let drained = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Extract], "w", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert!(db
        .knowledge_complete_job(drained.id, &drained.lease_token, None, None)
        .await
        .unwrap());

    // Failed terminal state blocks identically — no silent retry loops.
    let (failed_id, _) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-b"),
            Some("hash-3"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let claimed = db
        .knowledge_claim_next_job(&[super::types::KnowledgeJobKind::Extract], "w", 30_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, failed_id);
    assert!(db
        .knowledge_fail_job(
            claimed.id,
            &claimed.lease_token,
            "no_evidence",
            Some("窗口内没有可引用的证据"),
            false,
            0
        )
        .await
        .unwrap());
    let (refailed_id, refailed_created) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-b"),
            Some("hash-3"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(!refailed_created, "a failed job must block re-enqueueing");
    assert_eq!(refailed_id, failed_id);
    let failed = db.knowledge_get_job(failed_id).await.unwrap().unwrap();
    assert_eq!(
        failed.last_error_message.as_deref(),
        Some("窗口内没有可引用的证据")
    );

    // Manual retry remains the human path back into the queue; the retried
    // row is pending again and re-owns the partial unique slot.
    assert!(db.knowledge_retry_job(failed_id).await.unwrap());

    // `cancelled` jobs never block fresh work for their inputs: the partial
    // unique index only guards pending/running/paused rows, so a new row for
    // the same inputs can coexist with the cancelled one.
    let (cancelled_id, _) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-d"),
            Some("hash-4"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    sqlx::query("UPDATE knowledge_jobs SET state = 'cancelled' WHERE id = ?1")
        .bind(cancelled_id)
        .execute(&db.pool)
        .await
        .unwrap();
    let (_, after_cancel_created) = db
        .knowledge_enqueue_job(
            super::types::KnowledgeJobKind::Extract,
            Some("scope-d"),
            Some("hash-4"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(
        after_cancel_created,
        "cancelled inputs must be re-enqueueable"
    );
}

/// ④ 三机制的持久化底座（plan §4.2）：提名记录/刷新、蒸馏状态 upsert、
/// 候选 scope 按最早提名排序、成员查询只含 active 单元的最新有效修订。
#[tokio::test]
async fn distill_state_and_nominations_round_trip() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();

    // 从未蒸馏：无状态。
    assert!(db.knowledge_get_distill_state("arc|ticket").await.unwrap().is_none());

    // 提名：可记录、可刷新（同 unit 幂等更新时间与 flow）。
    db.knowledge_record_distill_nomination("arc|ticket", "wu-1", Some("工单处理"))
        .await
        .unwrap();
    db.knowledge_record_distill_nomination("arc|ticket", "wu-1", None)
        .await
        .unwrap();
    let (flow,): (Option<String>,) =
        sqlx::query_as("SELECT flow FROM knowledge_distill_nominations WHERE scope_key='arc|ticket' AND work_unit_id='wu-1'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(flow, None, "重提名单行覆盖");

    // 候选 scope：按最早提名时间排序，全量返回——④ 定时器只决定何时观察，
    // 不按数量筛选（plan §4.2）。
    db.knowledge_record_distill_nomination("arc|other", "wu-2", None)
        .await
        .unwrap();
    sqlx::query("UPDATE knowledge_distill_nominations SET nominated_at = '2026-09-12T01:00:00.000000Z' WHERE scope_key='arc|other'")
        .execute(&db.pool)
        .await
        .unwrap();
    db.knowledge_record_distill_nomination("arc|third", "wu-3", None)
        .await
        .unwrap();
    sqlx::query("UPDATE knowledge_distill_nominations SET nominated_at = '2026-09-12T03:00:00.000000Z' WHERE scope_key='arc|third'")
        .execute(&db.pool)
        .await
        .unwrap();
    let scopes = db.knowledge_list_distill_candidate_scopes().await.unwrap();
    assert_eq!(
        scopes,
        vec![
            "arc|other".to_string(),
            "arc|third".to_string(),
            "arc|ticket".to_string()
        ],
        "所有提名 scope 全量返回，无数量截断"
    );

    // 蒸馏状态 upsert + 清空提名。
    db.knowledge_record_distill_success("arc|ticket", "set-hash-1").await.unwrap();
    db.knowledge_record_distill_success("arc|ticket", "set-hash-2").await.unwrap();
    let state = db.knowledge_get_distill_state("arc|ticket").await.unwrap().unwrap();
    assert_eq!(state.0.as_deref(), Some("set-hash-2"), "后写覆盖");
    assert!(state.1.is_some());
    assert_eq!(db.knowledge_clear_distill_nominations("arc|ticket").await.unwrap(), 1);
    assert!(db.knowledge_list_distill_candidate_scopes().await.unwrap().contains(&"arc|other".to_string()));
    assert!(!db.knowledge_list_distill_candidate_scopes().await.unwrap().contains(&"arc|ticket".to_string()));
}

#[tokio::test]
async fn distill_members_reflect_active_units_and_latest_valid_revision() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let empty = db.knowledge_scope_distill_members("arc|empty").await.unwrap();
    assert!(empty.is_empty());

    let unit = db
        .knowledge_save_work_unit("arc|scope", None, Some("2026-09-12T09:00:00Z"), Some("2026-09-12T09:30:00Z"), "hash-a", "work_unit.v1", "zh-extract-v3", "{}")
        .await
        .unwrap();
    let members = db.knowledge_scope_distill_members("arc|scope").await.unwrap();
    assert_eq!(members, vec![(unit.clone(), "hash-a".to_string())]);

    // 新输入 → 同一逻辑单元上的新修订：成员集合按最新修订取值。
    // created_at 是毫秒精度的列默认值，两次快速插入可能同毫秒、令
    // ORDER BY 平局顺序不定——显式错开时间，只验证「取最新修订」语义。
    sqlx::query("UPDATE knowledge_work_unit_revisions SET created_at = '2026-09-12T00:00:00.000000Z' WHERE input_hash = 'hash-a'")
        .execute(&db.pool)
        .await
        .unwrap();
    db.knowledge_save_work_unit("arc|scope", None, Some("2026-09-12T09:00:00Z"), Some("2026-09-12T09:30:00Z"), "hash-b", "work_unit.v1", "zh-extract-v3", "{}")
        .await
        .unwrap();
    let members = db.knowledge_scope_distill_members("arc|scope").await.unwrap();
    assert_eq!(members.len(), 1, "同一逻辑单元只取最新有效修订");
    assert_eq!(members[0].0, unit);
    assert_eq!(members[0].1, "hash-b");
}
