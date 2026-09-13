// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Knowledge distillation delivery (plan §4.2, ④ 三机制投递).
//!
//! The ④ cadence replaces "extract success → compile" with a coarse timer
//! that only decides *when to look*. Whether a scope is delivered is decided
//! by three independent mechanisms — none of them a count threshold, none of
//! them a full sweep:
//!
//! 1. **模型提名** — the extract model flagged the scope's work as showing a
//!    reusable pattern (`knowledge_distill_nominations`; recorded by the same
//!    model call that produced the WorkUnit, no extra calls). A scope with no
//!    outstanding nomination is never a candidate.
//! 2. **变更驱动** — the scope's work-unit set hash
//!    ([`scope_work_unit_set_hash`]) must differ from the hash recorded at
//!    its last distillation; unchanged inputs would only re-burn budget on
//!    the same members.
//! 3. **冷却期** — the last distillation must be at least
//!    `knowledgeDistillCooldownDays` old.
//!
//! Delivery = enqueueing a Compile job whose input hash IS the set hash, so
//! the job queue's terminal-state dedup inherits the change semantics.

use std::sync::Arc;

use chrono::Utc;
use screenpipe_db::{fingerprint, DatabaseManager, KnowledgeJobKind};

use super::types::KnowledgeError;

/// Version tag for the work-unit set hash family.
pub const DISTILL_SET_HASH_VERSION: &str = "knowledge-distill-set-v1";

/// Why a candidate scope was not delivered on this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistillSkip {
    /// 机制一：没有待处理的模型提名。
    NotNominated,
    /// 机制三：距上次蒸馏仍在冷却期内。
    CooledDown,
    /// 机制二：自上次蒸馏以来 WorkUnit 集合没有变化。
    NoChange,
}

/// The scope's work-unit set identity: sorted `unit_id:input_hash` members
/// joined into one fingerprint. New/changed/replaced work units change the
/// hash; a pure re-read does not.
pub async fn scope_work_unit_set_hash(
    db: &DatabaseManager,
    scope: &str,
) -> Result<String, sqlx::Error> {
    let members = db.knowledge_scope_distill_members(scope).await?;
    Ok(distill_set_hash(scope, &members))
}

fn distill_set_hash(scope: &str, members: &[(String, String)]) -> String {
    let mut parts: Vec<String> = members
        .iter()
        .map(|(unit_id, input_hash)| format!("{unit_id}:{input_hash}"))
        .collect();
    parts.sort();
    fingerprint(&[DISTILL_SET_HASH_VERSION, scope, &parts.join(",")])
}

/// One scope's delivery decision ( nomination is checked by the caller via
/// the candidate list). `Ok(Some(hash))` means deliver a compile job keyed by
/// the current set hash; `Ok(None)` means skip for the returned reason.
pub async fn evaluate_scope(
    db: &DatabaseManager,
    scope: &str,
    cooldown_days: u64,
) -> Result<Result<String, DistillSkip>, KnowledgeError> {
    // 机制三（冷却期）first: cheaper than hashing, and the cooldown makes the
    // change check moot anyway.
    if let Some((_, last_distilled_at)) = db
        .knowledge_get_distill_state(scope)
        .await
        .map_err(|e| KnowledgeError::new("db_error", e.to_string(), true))?
    {
        if let Some(at) = last_distilled_at.as_deref().and_then(parse_ts) {
            let cooldown = chrono::Duration::days(cooldown_days as i64);
            if Utc::now() < at + cooldown {
                return Ok(Err(DistillSkip::CooledDown));
            }
        }
    }
    // 机制二（变更驱动）：与上次蒸馏以来的 WorkUnit 集合哈希比较。
    let hash = scope_work_unit_set_hash(db, scope)
        .await
        .map_err(|e| KnowledgeError::new("db_error", e.to_string(), true))?;
    if let Some((last_hash, _)) = db
        .knowledge_get_distill_state(scope)
        .await
        .map_err(|e| KnowledgeError::new("db_error", e.to_string(), true))?
    {
        if last_hash.as_deref() == Some(hash.as_str()) {
            return Ok(Err(DistillSkip::NoChange));
        }
    }
    Ok(Ok(hash))
}

/// The ④ tick: look at nominated scopes once per cadence and deliver every
/// one that passes all three mechanisms. The timer only decides *when to
/// look* — nomination, change and cooldown are the only filters, so there is
/// no per-round delivery cap (plan §4.2). Returns the number of compile jobs
/// created. `cooldown_days` = `knowledgeDistillCooldownDays`.
pub async fn discover_and_enqueue(
    db: &Arc<DatabaseManager>,
    cooldown_days: u64,
) -> Result<usize, KnowledgeError> {
    // 机制一（模型提名）：候选 scope 只来自提名表——模型决定值不值得；
    // 候选列表全量返回，不做数量截断。
    let scopes = db
        .knowledge_list_distill_candidate_scopes()
        .await
        .map_err(|e| KnowledgeError::new("db_error", e.to_string(), true))?;
    let mut created = 0usize;
    for scope in scopes {
        let hash = match evaluate_scope(db, &scope, cooldown_days).await? {
            Ok(hash) => hash,
            Err(reason) => {
                tracing::debug!(scope = %scope, ?reason, "distill: scope skipped");
                continue;
            }
        };
        match db
            .knowledge_enqueue_job(KnowledgeJobKind::Compile, Some(&scope), Some(&hash), None, None, None)
            .await
        {
            Ok((_id, true)) => {
                created += 1;
                tracing::info!(scope = %scope, "distill: enqueued compile job (nominated + changed + cooled)");
            }
            Ok(_) => {}
            Err(e) => return Err(KnowledgeError::new("db_error", e.to_string(), true)),
        }
    }
    Ok(created)
}

fn parse_ts(value: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::executor::{KnowledgeModelExecutor, ModelIdentity};
    use screenpipe_config::DbConfig;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio_util::sync::CancellationToken;

    async fn test_db() -> Arc<DatabaseManager> {
        Arc::new(
            DatabaseManager::new("sqlite::memory:", DbConfig::default())
                .await
                .unwrap(),
        )
    }

    /// Seed one active work unit (with one valid revision) in `scope` via the
    /// same API the extract handler commits through — no model involved.
    async fn seed_work_unit(db: &DatabaseManager, scope: &str, key: &str, body: &str) -> String {
        db.knowledge_save_work_unit(
            scope,
            None,
            Some(&format!("2026-09-12T0{key}:00:00Z")),
            Some(&format!("2026-09-12T0{key}:30:00Z")),
            &format!("{key}-input-hash"),
            "work_unit.v1",
            "zh-extract-v3",
            body,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn set_hash_tracks_member_changes_and_ignores_order() {
        let db = test_db().await;
        let scope = "arc|ticket 42";
        let a = seed_work_unit(&db, scope, "1", "{}").await;
        let base = scope_work_unit_set_hash(&db, scope).await.unwrap();

        // Same members → same hash (re-read stability).
        assert_eq!(scope_work_unit_set_hash(&db, scope).await.unwrap(), base);

        // A new work unit changes the hash (机制二：有新料才投).
        seed_work_unit(&db, scope, "2", "{}").await;
        let with_newcomer = scope_work_unit_set_hash(&db, scope).await.unwrap();
        assert_ne!(base, with_newcomer);

        // Scopes are isolated.
        seed_work_unit(&db, "arc|other", "3", "{}").await;
        assert_eq!(scope_work_unit_set_hash(&db, scope).await.unwrap(), with_newcomer);
        assert_ne!(
            distill_set_hash(scope, &[]),
            distill_set_hash("arc|other", &[]),
        );
        let _ = a;
    }

    #[tokio::test]
    async fn unnominated_scope_is_never_delivered() {
        let db = test_db().await;
        let scope = "arc|ticket 42";
        seed_work_unit(&db, scope, "1", "{}").await;
        // No nomination rows → the scope is not even a candidate.
        let created = discover_and_enqueue(&db, 7).await.unwrap();
        assert_eq!(created, 0, "未提名的 scope 不投递");
        let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_jobs")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(jobs, 0);
    }

    #[tokio::test]
    async fn nominated_but_unchanged_scope_is_not_delivered() {
        let db = test_db().await;
        let scope = "arc|ticket 42";
        seed_work_unit(&db, scope, "1", "{}").await;
        db.knowledge_record_distill_nomination(scope, "wu-1", Some("工单处理"))
            .await
            .unwrap();
        // First delivery passes (never distilled → change check passes).
        let created = discover_and_enqueue(&db, 7).await.unwrap();
        assert_eq!(created, 1);

        // Simulate a completed distillation recording the CURRENT set hash.
        let hash = scope_work_unit_set_hash(&db, scope).await.unwrap();
        db.knowledge_record_distill_success(scope, &hash).await.unwrap();
        db.knowledge_clear_distill_nominations(scope).await.unwrap();

        // A fresh nomination on unchanged members: 机制二 blocks — no new
        // material since the last distillation, no delivery.
        db.knowledge_record_distill_nomination(scope, "wu-1", None)
            .await
            .unwrap();
        let created = discover_and_enqueue(&db, 7).await.unwrap();
        assert_eq!(created, 0, "无变更不投递");
    }

    #[tokio::test]
    async fn cooled_down_scope_is_not_delivered() {
        let db = test_db().await;
        let scope = "arc|ticket 42";
        let unit = seed_work_unit(&db, scope, "1", "{}").await;
        db.knowledge_record_distill_nomination(scope, &unit, None)
            .await
            .unwrap();
        // Last distillation just now → cooldown blocks regardless of change.
        let hash = scope_work_unit_set_hash(&db, scope).await.unwrap();
        db.knowledge_record_distill_success(scope, &hash).await.unwrap();
        sqlx::query("UPDATE knowledge_distill_state SET last_distilled_at = ?1 WHERE scope_key = ?2")
            .bind(Utc::now().to_rfc3339())
            .bind(scope)
            .execute(&db.pool)
            .await
            .unwrap();
        db.knowledge_record_distill_nomination(scope, &unit, None)
            .await
            .unwrap();
        let decision = evaluate_scope(&db, scope, 7).await.unwrap();
        assert!(matches!(decision, Err(DistillSkip::CooledDown)));
        let created = discover_and_enqueue(&db, 7).await.unwrap();
        assert_eq!(created, 0, "冷却期内不投递");
    }

    #[tokio::test]
    async fn all_three_mechanisms_pass_together_exactly_once() {
        let db = test_db().await;
        let scope = "arc|ticket 42";
        let unit = seed_work_unit(&db, scope, "1", "{}").await;
        db.knowledge_record_distill_nomination(scope, &unit, Some("工单处理流程"))
            .await
            .unwrap();
        // 首次：提名 + 从未蒸馏（无冷却、无“无变化”）→ 投递。
        let created = discover_and_enqueue(&db, 7).await.unwrap();
        assert_eq!(created, 1);
        let (kind, input_hash): (String, String) =
            sqlx::query_as("SELECT kind, input_hash FROM knowledge_jobs")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(kind, "compile");
        assert_eq!(
            input_hash,
            scope_work_unit_set_hash(&db, scope).await.unwrap(),
            "compile 任务的身份就是当前集合哈希"
        );

        // Same inputs next tick: the succeeded job dedupes AND the change
        // check skips — zero extra deliveries.
        let created = discover_and_enqueue(&db, 7).await.unwrap();
        assert_eq!(created, 0);

        // New work unit + fresh nomination after the cooldown window: delivered.
        let hash = scope_work_unit_set_hash(&db, scope).await.unwrap();
        db.knowledge_record_distill_success(scope, &hash).await.unwrap();
        sqlx::query("UPDATE knowledge_distill_state SET last_distilled_at = ?1 WHERE scope_key = ?2")
            .bind((Utc::now() - chrono::Duration::days(8)).to_rfc3339())
            .bind(scope)
            .execute(&db.pool)
            .await
            .unwrap();
        let newcomer = seed_work_unit(&db, scope, "2", "{}").await;
        db.knowledge_record_distill_nomination(scope, &newcomer, None)
            .await
            .unwrap();
        let created = discover_and_enqueue(&db, 7).await.unwrap();
        assert_eq!(created, 1, "提名+变更+过冷却 → 投递");
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_jobs")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(total, 2);
    }

    #[tokio::test]
    async fn all_nominated_scopes_delivered_without_cap() {
        let db = test_db().await;
        // 25 scopes > the retired `knowledgeDiscoveryBatch` default (20): the
        // ④ tick must deliver every scope that passes the three mechanisms —
        // the timer decides when to look, never how many.
        for k in 0..25 {
            let scope = format!("arc|scope-{k}");
            let unit = seed_work_unit(&db, &scope, "1", "{}").await;
            db.knowledge_record_distill_nomination(&scope, &unit, None)
                .await
                .unwrap();
        }
        let created = discover_and_enqueue(&db, 7).await.unwrap();
        assert_eq!(created, 25, "满足三机制的 scope 全部投递，无数量阈值");
        let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_jobs")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(jobs, 25);
    }

    /// Guard the compile-side bookkeeping contract: on success the scope's
    /// distillation state records the current set hash and nominations clear.
    /// Driven through the real compile handler with a scripted executor.
    #[tokio::test]
    async fn compile_success_records_distill_state_and_clears_nominations() {
        use crate::knowledge::executor::{BudgetedExecutor, CompletionRequest};
        use crate::knowledge::worker::{JobContext, WorkerHandle};

        struct Scripted {
            calls: AtomicUsize,
            _prompts: Mutex<Vec<String>>,
        }
        impl KnowledgeModelExecutor for Scripted {
            fn identity(&self) -> ModelIdentity {
                ModelIdentity {
                    preset_id: "fixture".into(),
                    provider_catalog_id: "fixture".into(),
                    model_id: "fixture".into(),
                    wire_api: "openai-completions".into(),
                    pi_version: "test".into(),
                    profile_version: "knowledge-extract-v1".into(),
                }
            }
            fn complete(
                &self,
                _request: CompletionRequest,
                _cancel: CancellationToken,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, crate::knowledge::types::KnowledgeError>> + Send + '_>,
            > {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Box::pin(async {
                    Ok(r#"{"sop":null,"decision_rules":[],"exception_playbooks":[]}"#.to_string())
                })
            }
        }

        let db = test_db().await;
        let scope = "arc|ticket 42";
        for k in 1..=3 {
            seed_work_unit(&db, scope, &k.to_string(), "{}").await;
        }
        db.knowledge_record_distill_nomination(scope, "wu-any", None)
            .await
            .unwrap();

        let scripted = std::sync::Arc::new(Scripted {
            calls: AtomicUsize::new(0),
            _prompts: Mutex::new(Vec::new()),
        });
        let ctx = JobContext {
            db: db.clone(),
            executor: BudgetedExecutor::new(scripted.clone(), 3),
            cancel: CancellationToken::new(),
            lease_token: "lease".into(),
            job_id: 1,
            worker: WorkerHandle::for_tests(),
            scope_key: Some(scope.to_string()),
            input_hash: Some("set-hash".into()),
            payload: None,
        };
        let handler = super::super::compile::compile_handler();
        let outcome = handler(ctx)
            .await
            .unwrap_or_else(|failure| panic!("compile failed: {}", failure.code));
        assert!(outcome.result_ref.as_deref().unwrap_or("").starts_with("candidates="));

        let state = db.knowledge_get_distill_state(scope).await.unwrap().unwrap();
        let current = scope_work_unit_set_hash(&db, scope).await.unwrap();
        assert_eq!(state.0.as_deref(), Some(current.as_str()), "记录的是当前集合哈希");
        assert!(state.1.is_some(), "记录蒸馏时间");
        let nominations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_distill_nominations WHERE scope_key = ?1")
            .bind(scope)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(nominations, 0, "蒸馏完成后提名清空");
        assert_eq!(scripted.calls.load(Ordering::SeqCst), 1);
    }
}
