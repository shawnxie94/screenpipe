// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Knowledge compilation: work units in one scope → SOP / DecisionRule /
//! ExceptionPlaybook candidates via the strict completion, validated by the
//! type registry before anything is stored. No evidence, no knowledge.

use std::sync::Arc;

use serde_json::{json, Value};
use screenpipe_db::{KnowledgeJobKind, DatabaseManager};

use super::executor::{KnowledgeLimits, CompletionRequest};
use super::extract::parse_json_object;
use super::prompts;
use super::registry::{self, RegistryContext};
use super::sources::input_hash_for;
use super::types::KnowledgeType;
use super::worker::{JobContext, JobFailure, JobOutcome};

pub fn compile_handler() -> super::worker::HandlerFn {
    Arc::new(|ctx: JobContext| Box::pin(run_compile(ctx)))
}

/// 活动键口径：与活动层的间隔一一对应（`interval_start|interval_end`）。
/// 同一间隔重算产生的多个 WorkUnit 共享一个键，SOP 会话数因此只计一个；
/// 缺边界的（历史）WorkUnit 退化为 `wu:<work_unit_id>`，各自计一个会话。
fn activity_key(
    work_unit_id: &str,
    interval_start: Option<&str>,
    interval_end: Option<&str>,
) -> String {
    match (interval_start, interval_end) {
        (Some(start), Some(end)) if !start.is_empty() && !end.is_empty() => {
            format!("{start}|{end}")
        }
        _ => format!("wu:{work_unit_id}"),
    }
}

async fn run_compile(ctx: JobContext) -> Result<JobOutcome, JobFailure> {
    let scope = ctx.scope_key.clone().unwrap_or_default();

    // Existing work units in this scope (valid revisions only).
    let scope_units = ctx
        .db
        .knowledge_list_scope_work_units(&scope, 20)
        .await
        .map_err(|e| JobFailure::transient(format!("db_error:{e}")))?;
    if scope_units.units.len() < 3 {
        // Not enough independent sessions to propose anything yet; succeed
        // quietly — a future extract re-triggers compile.
        return Ok(JobOutcome { result_ref: Some("insufficient_sessions".into()), cursor: None });
    }

    // Build u1..uN reference map for the prompt + validation.
    let mut ref_map = std::collections::HashMap::new();
    let mut work_unit_activities = std::collections::HashMap::new();
    let mut work_unit_lines = Vec::new();
    for (i, (unit, revision)) in scope_units.units.iter().enumerate() {
        let ref_id = format!("u{}", i + 1);
        ref_map.insert(ref_id.clone(), unit.id.clone());
        work_unit_activities.insert(
            ref_id.clone(),
            activity_key(
                &unit.id,
                unit.interval_start.as_deref(),
                unit.interval_end.as_deref(),
            ),
        );
        let body: Value = serde_json::from_str(&revision.body).unwrap_or(Value::Null);
        work_unit_lines.push(format!(
            "[{ref_id}] ({})\n{}",
            unit
                .interval_start
                .clone()
                .unwrap_or_default(),
            serde_json::to_string(&body).unwrap_or_default()
        ));
    }

    // Existing knowledge in this scope (for dedupe context).
    let existing = json!({
        "sop": ctx.db.knowledge_find_knowledge_by_scope(&scope, "sop").await.ok().flatten().map(|k| k.id),
        "decision_rules": ctx.db.knowledge_find_knowledge_by_scope(&scope, "decision_rule").await.ok().flatten().map(|k| k.id),
        "exception_playbooks": ctx.db.knowledge_find_knowledge_by_scope(&scope, "exception_playbook").await.ok().flatten().map(|k| k.id),
    });

    let user_prompt = prompts::COMPILE_USER
        .replace("{scope}", &scope)
        .replace("{work_units}", &work_unit_lines.join("\n\n"))
        .replace("{existing}", &existing.to_string());

    let mut text = call_model(&ctx, &user_prompt).await?;
    let mut parsed = parse_json_object(&text);
    if parsed.is_none() {
        text = call_model(&ctx, &format!("你上次输出不是合法 JSON，请重新只输出 JSON。\n\n{user_prompt}")).await?;
        parsed = parse_json_object(&text);
    }
    let body = parsed.ok_or_else(|| JobFailure::permanent("invalid_output"))?;

    let reg_ctx = RegistryContext {
        work_unit_refs: &ref_map,
        work_unit_activities: &work_unit_activities,
    };
    let mut saved: Vec<String> = Vec::new();

    // SOP candidate: strict 3-session gate at the registry level.
    if let Some(sop) = body.get("sop").filter(|v| !v.is_null()) {
        match try_save(&ctx, KnowledgeType::Sop, sop, &reg_ctx, &ref_map, &scope).await {
            Ok(id) => saved.push(id),
            Err(SaveSkip::Invalid(reason)) => {
                tracing::info!("compile: SOP candidate rejected: {reason}");
            }
            Err(SaveSkip::Db(e)) => return Err(JobFailure::transient(format!("db_error:{e}"))),
        }
    }
    if let Some(rules) = body.get("decision_rules").and_then(Value::as_array) {
        for rule in rules {
            if let Ok(id) = try_save(&ctx, KnowledgeType::DecisionRule, rule, &reg_ctx, &ref_map, &scope).await {
                saved.push(id);
            }
        }
    }
    if let Some(playbooks) = body.get("exception_playbooks").and_then(Value::as_array) {
        for pb in playbooks {
            if let Ok(id) = try_save(&ctx, KnowledgeType::ExceptionPlaybook, pb, &reg_ctx, &ref_map, &scope).await {
                saved.push(id);
            }
        }
    }

    // ④ bookkeeping (§4.2 三机制): a distillation completed for this scope —
    // record the work-unit set hash + moment and drop the consumed
    // nominations, so the next delivery needs a fresh nomination AND changed
    // inputs AND an expired cooldown. Only reached past the
    // insufficient-sessions gate: that early return is not a distillation.
    let set_hash = super::distill::scope_work_unit_set_hash(&ctx.db, &scope)
        .await
        .map_err(|e| JobFailure::transient(format!("db_error:{e}")))?;
    ctx.db
        .knowledge_record_distill_success(&scope, &set_hash)
        .await
        .map_err(|e| JobFailure::transient(format!("db_error:{e}")))?;
    let _ = ctx.db.knowledge_clear_distill_nominations(&scope).await;

    Ok(JobOutcome {
        result_ref: Some(format!("candidates={}", saved.len())),
        cursor: ctx.input_hash.clone(),
    })
}

enum SaveSkip {
    Invalid(String),
    Db(String),
}

#[allow(clippy::too_many_arguments)]
async fn try_save(
    ctx: &JobContext,
    knowledge_type: KnowledgeType,
    body: &Value,
    reg_ctx: &RegistryContext<'_>,
    ref_map: &std::collections::HashMap<String, String>,
    scope: &str,
) -> Result<String, SaveSkip> {
    // Registry validation: existence + shape of every fact reference.
    registry::validate(knowledge_type, body, reg_ctx)
        .map_err(|e| SaveSkip::Invalid(format!("{}: {}", e.path, e.reason)))?;

    let title = body.get("title").and_then(Value::as_str).unwrap_or("未命名");
    // Compile input identity: the scope's work-unit set + prompt version +
    // the knowledge-distill skill body hash (skill edits recompute).
    let mut unit_ids: Vec<String> = ref_map.values().cloned().collect();
    unit_ids.sort();
    let input_hash = input_hash_for(
        &unit_ids.iter().map(|id| (id.clone(), String::new())).collect::<Vec<_>>(),
        &format!("{scope}:{}:{}", knowledge_type.as_str(), title),
        &super::skill_revisions::KNOWLEDGE_DISTILL_SKILL_REVISION,
    );

    // The knowledge depends on its work units AND transitively on every
    // source behind them (conservative whole-body deletion).
    #[allow(clippy::too_many_arguments)]
    async fn save(
        ctx_db: &Arc<DatabaseManager>,
        body: &Value,
        unit_ids: &[String],
        title: &str,
        knowledge_type: KnowledgeType,
        input_hash: &str,
        scope: &str,
    ) -> Result<(String, i64), sqlx::Error> {
        let body_str = body.to_string();
        let existing = ctx_db
            .knowledge_find_knowledge_by_scope(scope, knowledge_type.as_str())
            .await?;
        match existing {
            Some(item) => {
                ctx_db
                    .knowledge_add_knowledge_candidate_version(
                        &item.id,
                        title,
                        &body_str,
                        input_hash,
                        unit_ids,
                    )
                    .await
                    .map(|version| (item.id, version))
            }
            None => ctx_db
                .knowledge_create_knowledge_candidate(
                    knowledge_type.as_str(),
                    scope,
                    title,
                    &body_str,
                    input_hash,
                    unit_ids,
                )
                .await
                .map(|(id, version)| (id, version)),
        }
    }

    let (knowledge_id, knowledge_version_id) = save(
        &ctx.db,
        body,
        &unit_ids,
        title,
        knowledge_type,
        &input_hash,
        scope,
    )
    .await
    .map_err(|e| SaveSkip::Db(e.to_string()))?;
    // Transitive source edges: work unit → its sources → this knowledge.
    let knowledge_version = knowledge_version_id.to_string();
    for wu in ref_map.values() {
        let sources = ctx
            .db
            .knowledge_sources_of_consumer("work_unit", wu)
            .await
            .unwrap_or_default();
        for (source_uid, revision) in sources {
            let _ = ctx
                .db
                .knowledge_register_dependency(
                    "knowledge",
                    &knowledge_id,
                    Some(&knowledge_version),
                    None,
                    &source_uid,
                    revision.as_deref(),
                )
                .await;
        }
    }
    Ok(knowledge_id)
}

async fn call_model(ctx: &JobContext, user_prompt: &str) -> Result<String, JobFailure> {
    ctx.executor
        .complete(
            CompletionRequest {
                system: prompts::COMPILE_SYSTEM.to_string(),
                user_prompt: user_prompt.to_string(),
                max_output_tokens: KnowledgeLimits::MAX_OUTPUT_TOKENS,
                timeout: KnowledgeLimits::BACKGROUND_CALL_TIMEOUT,
                purpose: "compile",
            },
            ctx.cancel.clone(),
        )
        .await
        .map_err(|e| JobFailure::transient(e.code))
}

/// Queue-depth helper for `/knowledge/status` (candidates awaiting review).
pub async fn pending_candidate_count(db: &Arc<DatabaseManager>) -> u64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM knowledge_item_versions WHERE state = 'candidate'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap_or(0) as u64
}

#[allow(dead_code)]
fn keep_imports(_: KnowledgeJobKind) {}

#[cfg(test)]
mod tests {
    use super::activity_key;

    #[test]
    fn activity_key_binds_to_interval_and_degrades_without_bounds() {
        assert_eq!(
            activity_key(
                "wu-1",
                Some("2026-09-01T09:00:00Z"),
                Some("2026-09-01T10:00:00Z")
            ),
            "2026-09-01T09:00:00Z|2026-09-01T10:00:00Z"
        );
        // 缺任一边界或空串：退化为 wu:<id>，各自计一个会话。
        assert_eq!(
            activity_key("wu-2", None, Some("2026-09-01T10:00:00Z")),
            "wu:wu-2"
        );
        assert_eq!(activity_key("wu-3", Some(""), Some("")), "wu:wu-3");
    }
}
