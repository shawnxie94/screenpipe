// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Work Unit extraction: discover ready final intervals, register their
//! sources, run the two-phase strict completion — interval summaries first,
//! on-demand raw-evidence recall second — and commit field-cited WorkUnit
//! revisions. Any failure leaves the coverage cursor untouched.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use screenpipe_db::{compute_input_hash, fingerprint};
use screenpipe_db::{
    ActivitySummaryEvidenceRef, DatabaseManager, KnowledgeJobKind, KnowledgeSourceInput, SourceKind,
    SourceLocator,
};
use tokio_util::sync::CancellationToken;

use super::executor::{BudgetedExecutor, CompletionRequest, KnowledgeLimits};
use super::prompts;
use super::sources::{
    bounded_excerpt, register_capture_source, EXTRACTOR_SCHEMA_VERSION,
    EXTRACT_PROMPT_VERSION,
};
use super::types::KnowledgeError;
use super::worker::{JobContext, JobFailure, JobOutcome};

/// Scan for final intervals that have no extract job yet and enqueue them.
/// Returns the number of jobs created. Called periodically by the desktop
/// shell; safe to run concurrently (active-input uniqueness dedupes).
///
/// Discovery pre-screens (§4.1.4): immature targets never reach the queue.
/// An interval must be `final`, settled past the same grace the summarizer
/// uses, and carry retained evidence — so `no_evidence` can no longer be
/// produced by discovery and never becomes a model call. `limits.batch` caps
/// how many jobs one round may enqueue (§4.2, `knowledgeDiscoveryBatch`).
pub async fn discover_and_enqueue(
    db: &Arc<DatabaseManager>,
    since: DateTime<Utc>,
    limits: super::summarize::DiscoveryLimits,
) -> Result<usize, KnowledgeError> {
    let intervals = db
        .list_activity_ledger(since, Utc::now() + chrono::Duration::hours(1), false, false)
        .await
        .map_err(|e| KnowledgeError::new("db_error", e.to_string(), true))?;
    let settled_before =
        Utc::now() - chrono::Duration::minutes(super::summarize::SETTLED_GRACE_MINUTES);
    let mut created = 0usize;
    for interval in intervals {
        if created >= limits.batch.max(0) as usize {
            break;
        }
        // Only settled intervals compile into Work Units.
        if interval.state != "final" {
            continue;
        }
        let window = parse_interval_window(&interval)?;
        if window.end > settled_before {
            // Still inside the settle grace: activity may keep landing here.
            continue;
        }
        if interval.evidence_count == 0 {
            // No retained evidence: extraction would fail with `no_evidence`
            // before doing anything useful.
            continue;
        }
        let scope = interval_scope(&None, &Some(interval.title.clone()), &interval.app_name);
        let members = load_window_member_bounds(db, window.start, window.end).await?;
        let input_hash = discovery_input_hash(&scope, &members);
        let payload = json!({
            "interval_start": interval.start_at,
            "interval_end": interval.end_at,
        })
        .to_string();
        match db
            .knowledge_enqueue_job(
                KnowledgeJobKind::Extract,
                Some(&scope),
                Some(&input_hash),
                Some(&payload),
                None,
                None,
            )
            .await
        {
            Ok((_id, true)) => created += 1,
            Ok(_) => {}
            Err(e) => return Err(KnowledgeError::new("db_error", e.to_string(), true)),
        }
    }
    Ok(created)
}

struct IntervalWindow {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

fn parse_interval_window(interval: &screenpipe_db::ActivityIntervalRecord) -> Result<IntervalWindow, KnowledgeError> {
    let parse = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .map(|t| t.with_timezone(&Utc))
            .map_err(|e| KnowledgeError::new("bad_interval_ts", e.to_string(), false))
    };
    Ok(IntervalWindow {
        start: parse(&interval.start_at)?,
        end: parse(&interval.end_at)?,
    })
}

/// The job window's member-activity set: every interval the extract handler
/// will actually read (`start_at` inside the window — the same predicate as
/// `build_summary_pack`). §4.1.2: a new activity in the window must change
/// the enqueued hash, otherwise terminal-state dedup would silence the scope
/// forever after its first job finished.
async fn load_window_member_bounds(
    db: &DatabaseManager,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<(String, String, String)>, KnowledgeError> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT interval_key, start_at, end_at FROM activity_intervals_active \
         WHERE start_at >= ?1 AND start_at < ?2 ORDER BY start_at, interval_key",
    )
    .bind(start.to_rfc3339())
    .bind(end.to_rfc3339())
    .fetch_all(&db.pool)
    .await
    .map_err(|e| KnowledgeError::new("db_error", e.to_string(), true))?;
    Ok(rows)
}

/// Discovery-time input identity (§4.1.2), following
/// `summary_input_hash`'s sorted-join pattern: prompt/schema versions +
/// scope + the sorted `interval_key:start:end` member set + the work-unit
/// skill revision (so a skill edit invalidates queued jobs too).
fn discovery_input_hash(scope: &str, members: &[(String, String, String)]) -> String {
    let mut parts: Vec<String> = members
        .iter()
        .map(|(key, start, end)| format!("{key}:{start}:{end}"))
        .collect();
    parts.sort();
    fingerprint(&[
        EXTRACTOR_SCHEMA_VERSION,
        EXTRACT_PROMPT_VERSION,
        scope,
        &parts.join(","),
        super::skill_revisions::WORK_UNIT_SKILL_REVISION.as_str(),
    ])
}

/// Explainable grouping key: task title + app. Same-flow sessions land in
/// the same scope for SOP compilation; users can regroup later.
fn interval_scope(task_key: &Option<String>, task_title: &Option<String>, app: &Option<String>) -> String {
    let title = task_title.clone().unwrap_or_else(|| "未命名任务".into());
    format!(
        "{}|{}",
        app.clone().unwrap_or_else(|| "unknown".into()),
        title.trim().to_lowercase()
    )
}

/// The extract handler (registered for Extract and BackfillExtract).
pub fn extract_handler() -> super::worker::HandlerFn {
    Arc::new(|ctx: JobContext| Box::pin(run_extract(ctx)))
}

async fn run_extract(ctx: JobContext) -> Result<JobOutcome, JobFailure> {
    let payload: Value = ctx
        .payload
        .as_deref()
        .and_then(|p| serde_json::from_str(p).ok())
        .unwrap_or(Value::Null);
    let interval_start = payload
        .get("interval_start")
        .and_then(Value::as_str)
        .ok_or_else(|| JobFailure::permanent("bad_payload"))?;
    let interval_end = payload
        .get("interval_end")
        .and_then(Value::as_str)
        .ok_or_else(|| JobFailure::permanent("bad_payload"))?;
    let scope = ctx.scope_key.clone().unwrap_or_default();

    let committed = extract_interval(
        &ctx.db,
        &ctx.executor,
        ctx.cancel.clone(),
        &scope,
        interval_start,
        interval_end,
        ctx.input_hash.as_deref(),
    )
    .await
    .map_err(|failure| JobFailure {
        backoff_ms: if failure.transient { 1_000 } else { 0 },
        code: failure.code,
        message: failure.message,
        transient: failure.transient,
    })?;

    Ok(JobOutcome {
        result_ref: Some(committed.unit_id),
        cursor: Some(committed.input_hash),
    })
}

/// One committed extract step.
#[derive(Debug)]
pub struct ExtractCommitted {
    pub unit_id: String,
    pub input_hash: String,
    /// The model nominated this window as knowledge-worthy (④ 机制一).
    pub nominated: bool,
}

/// Failure of one extract step: `invalid_recall`/`invalid_work_unit`/
/// `invalid_output`/`no_evidence`/`input_deleted` (permanent) vs db and model
/// errors (retryable). `message` is the human-readable cause that lands in
/// `knowledge_jobs.last_error_message` (§4.1.3).
#[derive(Debug, Clone)]
pub struct ExtractFailure {
    pub code: String,
    pub message: Option<String>,
    pub transient: bool,
}

impl ExtractFailure {
    fn permanent(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: None,
            transient: false,
        }
    }

    fn with_message(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: Some(message.into()),
            transient: false,
        }
    }

    fn db(error: sqlx::Error) -> Self {
        Self {
            code: format!("db_error:{error}"),
            message: Some(error.to_string()),
            transient: true,
        }
    }
}

/// Placeholder filling `{recall_pack}` in the phase-1 prompt.
const PHASE_ONE_RECALL_NOTE: &str = "（第一阶段：暂无召回内容。请按第一阶段协议输出 work_unit 与 recall。）";

/// Phase-1 `recall` may name at most this many summary blocks.
const MAX_RECALL_INTERVALS: usize = 5;
/// Raw evidence budget per recalled interval (chars).
const RECALL_INTERVAL_CHARS: usize = 2_000;
/// Raw evidence budget across all recalled intervals (chars).
const RECALL_TOTAL_CHARS: usize = 8_000;

/// The extract step's engine, split out so tests can drive it without a
/// lease: summary pack → phase 1 (recall request) → phase 2 (recalled raw
/// evidence, only when the model asked) → validate → commit.
async fn extract_interval(
    db: &DatabaseManager,
    executor: &BudgetedExecutor,
    cancel: CancellationToken,
    scope: &str,
    start: &str,
    end: &str,
    expected_input_hash: Option<&str>,
) -> Result<ExtractCommitted, ExtractFailure> {
    // 0. Execution-time version recheck (§4.1.4, attempt 3): the job carries
    //    the discovery input hash it was queued with. Recompute the CURRENT
    //    active member hash for the same scope+window and refuse on mismatch
    //    — a job queued against a superseded version must not spend model
    //    budget on the new version's inputs (the replacement is discovered
    //    and queued on its own). Without a queued hash (backfill callers),
    //    fall back to requiring at least one active interval in the window.
    let (start_ts, end_ts) = parse_window(start, end)?;
    match expected_input_hash {
        Some(expected) => {
            let members = load_window_member_bounds(db, start_ts, end_ts)
                .await
                .map_err(|e| ExtractFailure {
                    code: e.code,
                    message: Some(e.message),
                    transient: e.retryable,
                })?;
            let current = discovery_input_hash(scope, &members);
            if current != expected {
                return Err(ExtractFailure::with_message(
                    "superseded_input",
                    "排队的输入版本已被台账重建取代，任务作废；新版本会在后续发现中重新入队",
                ));
            }
        }
        None => {
            if !db
                .activity_window_has_active_intervals(start_ts, end_ts)
                .await
                .map_err(ExtractFailure::db)?
            {
                return Err(ExtractFailure::with_message(
                    "superseded_window",
                    "窗口内的活动已被更新的台账版本覆盖，任务作废；新版本会在后续发现中重新入队",
                ));
            }
        }
    }
    // 1. Summary-first pack over the window; intervals without a summary
    //    degrade to the raw evidence pack inside the same prompt. Sources are
    //    registered on the fly — identity AND deletion dependencies stay on
    //    the raw evidence even when the prompt shows only summaries.
    let pack = build_summary_pack(db, start, end).await?;
    if pack.sources.is_empty() {
        return Err(ExtractFailure::with_message(
            "no_evidence",
            "窗口内没有可引用的证据行（原始内容可能已被保留策略删除或为空）",
        ));
    }

    // 2. Input identity comes from the ACTUAL sources — must match the job
    //    hash family; new evidence on the same interval creates a revision.
    //    The work-unit skill body hash rides along so skill edits recompute.
    let input_hash = compute_input_hash(
        &pack.sources,
        scope,
        "activity-v1",
        EXTRACTOR_SCHEMA_VERSION,
        EXTRACT_PROMPT_VERSION,
        &super::skill_revisions::WORK_UNIT_SKILL_REVISION,
    );

    // 3. Phase 1: summary pack + recall request (one repair pass, budget
    //    owned by the worker: ≤3 calls across both phases).
    let user_prompt = prompts::EXTRACT_USER
        .replace("{window}", &format!("{start} ~ {end}"))
        .replace("{scope}", scope)
        .replace("{evidence_pack}", &pack.prompt_text)
        .replace("{recall_pack}", PHASE_ONE_RECALL_NOTE);
    let mut text = call_model(executor, cancel.clone(), &user_prompt).await?;
    let mut parsed = parse_json_object(&text);
    if !parsed.as_ref().is_some_and(|v| stage1_ready(v)) {
        text = call_model(executor, cancel.clone(), &repair_prompt(&text)).await?;
        parsed = parse_json_object(&text);
    }
    let stage1 = parsed.ok_or_else(|| ExtractFailure::permanent("invalid_output"))?;
    if !stage1_ready(&stage1) {
        return Err(ExtractFailure::permanent("invalid_output"));
    }
    // ④ 机制一（模型提名，§4.2）：同一调用附带的可选 `knowledge_nomination`。
    // 字段缺失或形状不对一律视为未提名，绝不阻断抽取。
    let mut nomination = parse_nomination(&stage1);

    // 4. Recall ids: only summary blocks the model actually saw. Anything
    //    else is rejected outright — no second call on fabricated ids.
    let recall = parse_recall(stage1.get("recall"), &pack.summary_ids)?;

    // 5. Phase 2 runs only when the model named intervals to recall.
    let body = if recall.is_empty() {
        unwrap_work_unit(&stage1).ok_or_else(|| ExtractFailure::permanent("invalid_output"))?
    } else {
        let recall_prompt = prompts::EXTRACT_USER
            .replace("{window}", &format!("{start} ~ {end}"))
            .replace("{scope}", scope)
            .replace("{evidence_pack}", &pack.prompt_text)
            .replace("{recall_pack}", &build_recall_pack(&pack, &recall));
        let mut text = call_model(executor, cancel.clone(), &recall_prompt).await?;
        let mut parsed = parse_json_object(&text);
        if parsed.is_none() {
            text = call_model(executor, cancel.clone(), &repair_prompt(&text)).await?;
            parsed = parse_json_object(&text);
        }
        let final_output = parsed.ok_or_else(|| ExtractFailure::permanent("invalid_output"))?;
        if nomination.is_none() {
            nomination = parse_nomination(&final_output);
        }
        unwrap_work_unit(&final_output).ok_or_else(|| ExtractFailure::permanent("invalid_output"))?
    };
    // 提名不进 WorkUnit 正文：正文会被 compile 原样拼进提示词。
    let body = strip_nomination(body);
    let nominated = nomination.as_ref().is_some_and(|n| n.nominated);

    // 6. Validate field-level evidence refs against this pack.
    validate_work_unit(&body, &pack.ref_ids)
        .map_err(|reason| ExtractFailure::permanent(format!("invalid_work_unit:{reason}")))?;

    // 7. Commit revision + register dependencies on the real sources.
    let unit_id = db
        .knowledge_save_work_unit(
            scope,
            None,
            Some(start),
            Some(end),
            &input_hash,
            EXTRACTOR_SCHEMA_VERSION,
            EXTRACT_PROMPT_VERSION,
            &body.to_string(),
        )
        .await
        .map_err(ExtractFailure::db)?;
    for (uid, _rev) in &pack.sources {
        let _ = db
            .knowledge_register_dependency("work_unit", &unit_id, Some(&input_hash), None, uid, None)
            .await;
    }

    // 8. ④ 机制一落库：被提名的 scope 记一条待蒸馏提名（§4.2）。编译不再由
    //    抽取即时入队——投递改由 ④ tick 的三机制（提名/变更/冷却）决定。
    //    记账失败不回滚抽取结果，只记日志。
    if nominated {
        let flow = nomination.and_then(|n| n.flow);
        if let Err(error) = db
            .knowledge_record_distill_nomination(scope, &unit_id, flow.as_deref())
            .await
        {
            tracing::warn!(unit = %unit_id, error = %error, "extract: recording knowledge nomination failed");
        }
    }

    Ok(ExtractCommitted { unit_id, input_hash, nominated })
}

/// The optional extraction-time nomination (④ 机制一): the model's same-call
/// verdict on whether this window shows a reusable, knowledge-worthy pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct KnowledgeNomination {
    pub nominated: bool,
    /// Suggested knowledge home (flow name); may be absent.
    pub flow: Option<String>,
}

/// Read `knowledge_nomination` from an extraction output. Missing field,
/// wrong shape or missing `nominated` bool all mean "not nominated" — the
/// field is optional and must never fail an otherwise valid extraction.
fn parse_nomination(output: &Value) -> Option<KnowledgeNomination> {
    let obj = output.get("knowledge_nomination")?.as_object()?;
    let nominated = obj.get("nominated").and_then(Value::as_bool)?;
    let flow = obj
        .get("flow")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|flow| !flow.is_empty())
        .map(str::to_owned);
    Some(KnowledgeNomination { nominated, flow })
}

/// Remove a nested `knowledge_nomination` from the WorkUnit body itself (in
/// case the model put it inside `work_unit` instead of the outer object), so
/// compile prompts never carry delivery bookkeeping.
fn strip_nomination(mut body: Value) -> Value {
    if let Some(obj) = body.as_object_mut() {
        obj.remove("knowledge_nomination");
    }
    body
}

struct SummaryPack {
    /// (source_uid, revision) of every registered evidence row in the
    /// window — input identity AND deletion dependencies, even when the
    /// prompt only shows summaries.
    sources: Vec<(String, String)>,
    /// Every citable id: eN → source_uid, sN → interval id (string).
    ref_ids: HashMap<String, String>,
    /// sN → interval_id, for on-demand recall.
    summary_ids: HashMap<String, i64>,
    /// (source_type, source_id) → eN label, to label recalled rows.
    row_refs: HashMap<(String, i64), String>,
    /// Registered evidence rows in presentation order.
    rows: Vec<RegisteredRow>,
    prompt_text: String,
}

struct RegisteredRow {
    interval_id: i64,
    kind: String,
    source_id: i64,
    occurred_at: String,
    source_uid: String,
    revision: String,
    /// Whitespace-normalized original text (possibly empty).
    text: String,
}

const MAX_SOURCES: usize = 32;
const MAX_PACK_CHARS: usize = 24_000;
/// Per-evidence excerpt cap before the total budget applies.
const PER_ITEM_CHARS: usize = 3_000;
/// Summary blocks shown for one window.
const MAX_SUMMARY_INTERVALS: usize = 40;
/// Summary text cap per block (the `long` band maximum).
const SUMMARY_EXCERPT_CHARS: usize = 600;
/// Reserved for truncation/sampling notes so the final pack stays in budget.
const PACK_NOTE_HEADROOM: usize = 256;

/// Summary-first input pack (B03a): one `sN` block per summarized interval
/// (time, app/title, summary, keywords, retention accounting, ≤3 evidence
/// citations), bounded by interval count, summary excerpt and total chars —
/// over-limit windows are sampled evenly by time and noted. Intervals
/// without a summary degrade to the raw evidence pack as an `[原始证据]`
/// section; a window with no summaries at all falls back to the original
/// evidence pack entirely.
async fn build_summary_pack(db: &DatabaseManager, start: &str, end: &str) -> Result<SummaryPack, ExtractFailure> {
    let (start_ts, end_ts) = parse_window(start, end)?;
    let intervals = load_window_intervals(db, start_ts, end_ts).await?;
    if !intervals.iter().any(|i| i.summary.is_some()) {
        // 过渡期回落：整窗没有摘要，退化为原始证据包（保留原路径语义）。
        let mut pack = build_evidence_pack(db, start_ts, end_ts).await?;
        if !pack.prompt_text.is_empty() {
            pack.prompt_text = format!("[原始证据]\n{}", pack.prompt_text);
        }
        return Ok(pack);
    }

    let rows = load_and_register_window_rows(db, start_ts, end_ts).await?;

    // Stable eN labels for every registered row, so summary citations and
    // recalled raw evidence speak the same id space.
    let mut ref_ids = HashMap::new();
    let mut row_refs = HashMap::new();
    let mut row_index: HashMap<(String, i64), &RegisteredRow> = HashMap::new();
    for (idx, row) in rows.iter().enumerate() {
        let label = format!("e{}", idx + 1);
        ref_ids.insert(label.clone(), row.source_uid.clone());
        row_refs.insert((row.source_type_key(), row.source_id), label);
        row_index.insert((row.source_type_key(), row.source_id), row);
    }

    let retention = load_retention_lines(db, start_ts, end_ts).await?;
    let summarized: Vec<&WindowInterval> = intervals.iter().filter(|i| i.summary.is_some()).collect();
    let (picked, sampled) = sample_uniform(summarized, MAX_SUMMARY_INTERVALS);

    let mut blocks = Vec::new();
    let mut summary_ids = HashMap::new();
    for (n, interval) in picked.iter().enumerate() {
        let sid = format!("s{}", n + 1);
        summary_ids.insert(sid.clone(), interval.id);
        let summary_text = bounded_excerpt(interval.summary.as_deref().unwrap_or(""), SUMMARY_EXCERPT_CHARS)
            .unwrap_or_default();
        let mut block = format!(
            "[{sid}] {} ~ {}｜{}｜{}\n摘要：{}\n关键字：{}\n采样记账：{}",
            interval.start_at,
            interval.end_at,
            interval.app_name.as_deref().unwrap_or("未知"),
            interval.title.trim(),
            summary_text,
            if interval.keywords.is_empty() {
                "无".to_string()
            } else {
                interval.keywords.join("、")
            },
            retention.get(&interval.id).map(String::as_str).unwrap_or("无记录"),
        );
        let citations: Vec<String> = interval
            .evidence_refs
            .iter()
            .filter_map(|r| {
                let key = (r.source_type.clone(), r.source_id);
                let label = row_refs.get(&key)?;
                let occurred_at = row_index.get(&key).map(|row| row.occurred_at.as_str()).unwrap_or("");
                Some(format!("[{label}] ({} #{} @ {})", r.source_type, r.source_id, occurred_at))
            })
            .collect();
        if !citations.is_empty() {
            block.push_str(&format!("\n引证：{}", citations.join("、")));
        }
        blocks.push(block);
    }
    for (sid, interval_id) in &summary_ids {
        ref_ids.insert(sid.clone(), interval_id.to_string());
    }

    let budget = MAX_PACK_CHARS - PACK_NOTE_HEADROOM;
    let mut block_text = blocks.join("\n\n");
    if block_text.len() > budget && blocks.len() > 1 {
        let keep = ((blocks.len() as u64) * (budget as u64) / (block_text.len() as u64)).max(1) as usize;
        let (blocks, _) = sample_uniform(blocks, keep.min(MAX_SUMMARY_INTERVALS));
        block_text = blocks.join("\n\n");
    }
    if sampled {
        block_text = format!("（窗口内间隔超过上限，已按时间均匀取样）\n{block_text}");
    }

    // Raw evidence for the summary-less intervals, inside the same prompt.
    let summarized_ids: HashSet<i64> = intervals
        .iter()
        .filter(|i| i.summary.is_some())
        .map(|i| i.id)
        .collect();
    let mut sections = Vec::new();
    let mut used_chars = block_text.len();
    let mut truncated = false;
    for row in &rows {
        if summarized_ids.contains(&row.interval_id) {
            continue;
        }
        let excerpt = bounded_excerpt(&row.text, PER_ITEM_CHARS).unwrap_or_default();
        if excerpt.is_empty() {
            continue;
        }
        if used_chars + excerpt.len() > budget {
            truncated = true;
            break;
        }
        used_chars += excerpt.len();
        let label = row_refs
            .get(&(row.source_type_key(), row.source_id))
            .cloned()
            .unwrap_or_default();
        sections.push(format!("[{label}] ({}) {excerpt}", row.kind));
    }

    let mut prompt_text = block_text;
    if !sections.is_empty() {
        prompt_text.push_str("\n\n[原始证据]\n");
        prompt_text.push_str(&sections.join("\n"));
    }
    if truncated {
        prompt_text.push_str("\n（原始证据超出整包上限，已截断）");
    }

    Ok(SummaryPack {
        sources: rows
            .iter()
            .map(|row| (row.source_uid.clone(), row.revision.clone()))
            .collect(),
        ref_ids,
        summary_ids,
        row_refs,
        rows,
        prompt_text,
    })
}

/// Fallback path (B03a rollback / no-summary window): the original raw
/// evidence pack, semantics unchanged.
async fn build_evidence_pack(
    db: &DatabaseManager,
    start_ts: DateTime<Utc>,
    end_ts: DateTime<Utc>,
) -> Result<SummaryPack, ExtractFailure> {
    let rows = load_and_register_window_rows(db, start_ts, end_ts).await?;

    let mut sources = Vec::new();
    let mut ref_ids = HashMap::new();
    let mut sections: Vec<String> = Vec::new();
    let mut used_chars = 0usize;

    for row in &rows {
        let excerpt = bounded_excerpt(&row.text, PER_ITEM_CHARS).unwrap_or_default();
        if used_chars + excerpt.len() > MAX_PACK_CHARS {
            break;
        }
        used_chars += excerpt.len();
        let ref_id = format!("e{}", sections.len() + 1);
        ref_ids.insert(ref_id.clone(), row.source_uid.clone());
        sources.push((row.source_uid.clone(), row.revision.clone()));
        sections.push(format!("[{ref_id}] ({}) {excerpt}", row.kind));
    }

    Ok(SummaryPack {
        sources,
        ref_ids,
        summary_ids: HashMap::new(),
        row_refs: HashMap::new(),
        rows,
        prompt_text: sections.join("\n"),
    })
}

/// One interval with its persisted summary when present.
struct WindowInterval {
    id: i64,
    start_at: String,
    end_at: String,
    app_name: Option<String>,
    title: String,
    summary: Option<String>,
    keywords: Vec<String>,
    evidence_refs: Vec<ActivitySummaryEvidenceRef>,
}

async fn load_window_intervals(
    db: &DatabaseManager,
    start_ts: DateTime<Utc>,
    end_ts: DateTime<Utc>,
) -> Result<Vec<WindowInterval>, ExtractFailure> {
    let rows: Vec<(i64, String, String, Option<String>, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT i.id, i.start_at, i.end_at, t.app_name, t.title, s.summary, s.keywords, s.evidence_refs \
             FROM activity_intervals_active i \
             JOIN activity_tasks t ON t.id = i.task_id \
             LEFT JOIN activity_interval_summaries s ON s.interval_id = i.id \
             WHERE i.start_at >= ?1 AND i.start_at < ?2 \
             ORDER BY i.start_at, i.id",
        )
        .bind(start_ts.to_rfc3339())
        .bind(end_ts.to_rfc3339())
        .fetch_all(&db.pool)
        .await
        .map_err(ExtractFailure::db)?;
    Ok(rows
        .into_iter()
        .map(|(id, start_at, end_at, app_name, title, summary, keywords, refs)| WindowInterval {
            id,
            start_at,
            end_at,
            app_name,
            title,
            summary,
            keywords: keywords
                .and_then(|k| serde_json::from_str(&k).ok())
                .unwrap_or_default(),
            evidence_refs: refs
                .and_then(|r| serde_json::from_str(&r).ok())
                .unwrap_or_default(),
        })
        .collect())
}

/// Per-interval retention accounting lines for the summary blocks.
async fn load_retention_lines(
    db: &DatabaseManager,
    start_ts: DateTime<Utc>,
    end_ts: DateTime<Utc>,
) -> Result<HashMap<i64, String>, ExtractFailure> {
    let rows: Vec<(i64, String, i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT r.interval_id, r.source_type, r.kept, r.dropped, r.drop_reason \
         FROM activity_interval_retention r \
         JOIN activity_intervals_active i ON i.id = r.interval_id \
         WHERE i.start_at >= ?1 AND i.start_at < ?2 \
         ORDER BY r.interval_id, r.source_type",
    )
    .bind(start_ts.to_rfc3339())
    .bind(end_ts.to_rfc3339())
    .fetch_all(&db.pool)
    .await
    .map_err(ExtractFailure::db)?;
    let mut grouped: HashMap<i64, Vec<String>> = HashMap::new();
    for (interval_id, source_type, kept, dropped, reason) in rows {
        let reason = reason
            .as_deref()
            .map(|r| format!("（{r}）"))
            .unwrap_or_default();
        grouped
            .entry(interval_id)
            .or_default()
            .push(format!("{source_type}: 保留 {kept}，丢弃 {dropped}{reason}"));
    }
    Ok(grouped
        .into_iter()
        .map(|(interval_id, lines)| (interval_id, lines.join("；")))
        .collect())
}

/// Load the window's retained evidence rows fresh and register each one as a
/// stable source. Deletion semantics: the locator row gone means the original
/// is deleted — refuse, never model. Sources feed input identity AND
/// deletion dependencies, so they are registered even when the prompt only
/// shows summaries.
async fn load_and_register_window_rows(
    db: &DatabaseManager,
    start_ts: DateTime<Utc>,
    end_ts: DateTime<Utc>,
) -> Result<Vec<RegisteredRow>, ExtractFailure> {
    let rows: Vec<(i64, String, i64, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ae.interval_id, ae.source_type, ae.source_id, ae.occurred_at, \
                COALESCE(sf.app_name, ue.app_name, ef.app_name), \
                COALESCE(sf.window_name, ue.window_title, ef.window_name) \
         FROM activity_evidence ae \
         JOIN activity_intervals_active i ON i.id = ae.interval_id \
         LEFT JOIN frames sf ON ae.source_type = 'frame' AND sf.id = ae.source_id \
         LEFT JOIN ui_events ue ON ae.source_type = 'ui_event' AND ue.id = ae.source_id \
         LEFT JOIN frames ef ON ue.frame_id = ef.id \
         WHERE i.start_at >= ?1 AND i.start_at < ?2 \
         ORDER BY ae.occurred_at LIMIT ?3",
    )
    .bind(start_ts.to_rfc3339())
    .bind(end_ts.to_rfc3339())
    .bind(MAX_SOURCES as i64)
    .fetch_all(&db.pool)
    .await
    .map_err(ExtractFailure::db)?;

    let mut registered = Vec::new();
    for (interval_id, kind, id, occurred_at, app_name, window_name) in rows {
        let (source_kind, locator_table) = match kind.as_str() {
            "frame" => (SourceKind::Frame, "frames"),
            "ui_event" => (SourceKind::UiEvent, "ui_events"),
            "audio" => (SourceKind::Audio, "audio_transcriptions"),
            _ => continue,
        };
        let alive_sql = format!("SELECT EXISTS(SELECT 1 FROM {locator_table} WHERE id = ?1)");
        let alive: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(alive_sql))
            .bind(id)
            .fetch_one(&db.pool)
            .await
            .unwrap_or(0);
        if alive == 0 {
            return Err(ExtractFailure::permanent("input_deleted"));
        }
        let text: Option<String> = match kind.as_str() {
            "frame" => sqlx::query_scalar::<_, String>("SELECT o.text FROM ocr_text o WHERE o.frame_id = ?1 LIMIT 1")
                .bind(id)
                .fetch_optional(&db.pool)
                .await
                .ok()
                .flatten(),
            "ui_event" => sqlx::query_scalar::<_, String>("SELECT COALESCE(text_content, element_value, element_name, event_type) FROM ui_events WHERE id = ?1")
                .bind(id)
                .fetch_optional(&db.pool)
                .await
                .ok()
                .flatten(),
            "audio" => sqlx::query_scalar::<_, String>("SELECT transcription FROM audio_transcriptions WHERE id = ?1")
                .bind(id)
                .fetch_optional(&db.pool)
                .await
                .ok()
                .flatten(),
            _ => None,
        };
        let Some(text) = text else {
            continue;
        };
        let normalized = super::sources::normalize_text(&text);
        let captured_at = DateTime::parse_from_rfc3339(&occurred_at)
            .map(|value| value.with_timezone(&Utc))
            .unwrap_or(start_ts);
        let registration = register_capture_source(
            db,
            source_kind,
            SourceLocator::new(locator_table, id),
            Some(&normalized),
            app_name.as_deref(),
            window_name.as_deref(),
            captured_at,
            source_kind == SourceKind::Frame,
        )
        .await
        .map_err(ExtractFailure::db)?;
        if registration.suppressed {
            continue;
        }
        registered.push(RegisteredRow {
            interval_id,
            kind,
            source_id: id,
            occurred_at,
            source_uid: registration.source_uid.clone(),
            revision: registration.revision.clone(),
            text: normalized,
        });
    }
    Ok(registered)
}

impl RegisteredRow {
    fn source_type_key(&self) -> String {
        self.kind.clone()
    }
}

/// Raw evidence text for the recalled intervals: ≤2000 chars per interval,
/// ≤8000 in total, labeled with the same eN ids as the pack so citations
/// survive into the final WorkUnit. The trailing instruction makes the
/// phase-2 contract explicit.
fn build_recall_pack(pack: &SummaryPack, recall: &[String]) -> String {
    let mut sections = Vec::new();
    let mut used = 0usize;
    let mut truncated = false;
    for id in recall {
        if used >= RECALL_TOTAL_CHARS {
            truncated = true;
            break;
        }
        let Some(interval_id) = pack.summary_ids.get(id) else {
            continue;
        };
        let mut budget = RECALL_INTERVAL_CHARS.min(RECALL_TOTAL_CHARS - used);
        let mut lines: Vec<String> = Vec::new();
        for row in &pack.rows {
            if row.interval_id != *interval_id {
                continue;
            }
            if budget == 0 {
                truncated = true;
                break;
            }
            let Some(label) = pack.row_refs.get(&(row.source_type_key(), row.source_id)) else {
                continue;
            };
            let Some(excerpt) = bounded_excerpt(&row.text, budget) else {
                continue;
            };
            budget -= excerpt.chars().count();
            used += excerpt.chars().count();
            lines.push(format!("[{label}] ({}) {excerpt}", row.kind));
        }
        if !lines.is_empty() {
            sections.push(format!("[{id}] 的原始证据：\n{}", lines.join("\n")));
        }
    }
    let mut out = sections.join("\n\n");
    if out.is_empty() {
        out.push_str("（被点名间隔没有可展示的原始证据文本）");
    }
    if truncated {
        out.push_str("\n（召回内容超出每间隔 2000 字符或总量 8000 字符上限，已截断）");
    }
    format!(
        "{out}\n\n以上是召回的原始证据。请输出最终 WorkUnit JSON（schema_version 为 2），不得再包含 recall 字段。只输出 JSON。"
    )
}

/// Accept the phase contract `{"work_unit": {…}, "recall": […]}` and also a
/// bare WorkUnit object that skipped the wrapper.
fn unwrap_work_unit(output: &Value) -> Option<Value> {
    if let Some(inner) = output.get("work_unit").filter(|v| v.is_object()) {
        return Some(inner.clone());
    }
    if output.get("schema_version").is_some() {
        return Some(output.clone());
    }
    None
}

/// Phase-1 output is usable when it either carries a WorkUnit (recall empty)
/// or names at least one summary block to recall.
fn stage1_ready(output: &Value) -> bool {
    output.is_object()
        && (unwrap_work_unit(output).is_some()
            || output
                .get("recall")
                .and_then(Value::as_array)
                .is_some_and(|r| !r.is_empty()))
}

/// Phase-1 `recall`: 0–5 ids, each of which must name a summary block in the
/// pack. Wrong shape, unknown ids or going over the cap are rejected outright
/// — no second call on ids the model never saw.
fn parse_recall(
    recall: Option<&Value>,
    summary_ids: &HashMap<String, i64>,
) -> Result<Vec<String>, ExtractFailure> {
    let Some(recall) = recall else {
        return Ok(Vec::new());
    };
    let items = recall
        .as_array()
        .ok_or_else(|| ExtractFailure::permanent("invalid_recall:recall 必须是字符串数组"))?;
    if items.len() > MAX_RECALL_INTERVALS {
        return Err(ExtractFailure::permanent(format!(
            "invalid_recall:recall {} 个，超出 0–{MAX_RECALL_INTERVALS}",
            items.len()
        )));
    }
    let mut out = Vec::new();
    for item in items {
        let id = item.as_str().unwrap_or_default();
        if !summary_ids.contains_key(id) {
            return Err(ExtractFailure::permanent(format!("invalid_recall:{id} 不是包内间隔编号")));
        }
        if !out.iter().any(|seen: &String| seen.as_str() == id) {
            out.push(id.to_string());
        }
    }
    Ok(out)
}

async fn call_model(
    executor: &BudgetedExecutor,
    cancel: CancellationToken,
    user_prompt: &str,
) -> Result<String, ExtractFailure> {
    executor
        .complete(
            CompletionRequest {
                system: prompts::EXTRACT_SYSTEM.to_string(),
                user_prompt: user_prompt.to_string(),
                max_output_tokens: KnowledgeLimits::MAX_OUTPUT_TOKENS,
                timeout: KnowledgeLimits::BACKGROUND_CALL_TIMEOUT,
                purpose: "extract",
            },
            cancel,
        )
        .await
        .map_err(|e| ExtractFailure {
            code: e.code,
            message: Some(e.message),
            transient: true,
        })
}

fn repair_prompt(bad_output: &str) -> String {
    format!(
        "你上一次的输出不是合法 JSON 或不符合要求。以下是你上次输出的前 2000 字符：\n{}\n\n请重新输出符合两阶段协议要求的 JSON（第一阶段 {{\"work_unit\":…,\"recall\":[…]}}；第二阶段为最终 WorkUnit JSON）。只输出 JSON。",
        bad_output.chars().take(2000).collect::<String>()
    )
}

fn parse_window(start: &str, end: &str) -> Result<(DateTime<Utc>, DateTime<Utc>), ExtractFailure> {
    let start_ts = DateTime::parse_from_rfc3339(start)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| ExtractFailure::permanent("bad_payload"))?;
    let end_ts = DateTime::parse_from_rfc3339(end)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| ExtractFailure::permanent("bad_payload"))?;
    Ok((start_ts, end_ts))
}

/// Evenly spaced sample over a time-ordered list, keeping both endpoints.
fn sample_uniform<T>(items: Vec<T>, cap: usize) -> (Vec<T>, bool) {
    let n = items.len();
    if cap == 0 || n <= cap {
        return (items, false);
    }
    let keep: HashSet<usize> = if cap == 1 {
        [0usize].into_iter().collect()
    } else {
        (0..cap).map(|k| k * (n - 1) / (cap - 1)).collect()
    };
    let out = items
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep.contains(i))
        .map(|(_, v)| v)
        .collect();
    (out, true)
}

/// First balanced `{...}` in the text, honoring string literals and escapes.
/// Used when a runtime wraps its JSON object in prose or code fences.
fn first_balanced_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut start: Option<usize> = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&text[start.unwrap_or(0)..=i]);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn parse_json_object(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    let stripped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let stripped = stripped.strip_suffix("```").unwrap_or(stripped);
    if let Ok(value) = serde_json::from_str::<Value>(stripped.trim()) {
        if value.is_object() {
            return Some(value);
        }
    }
    let value: Value = serde_json::from_str(first_balanced_object(trimmed)?).ok()?;
    if value.is_object() {
        Some(value)
    } else {
        None
    }
}

/// WorkUnit schema v2: `process`/`environment`/`details` follow the same list
/// rules as the v1 lists — non-empty `value`, `evidence_refs` present and
/// hitting pack ids — and are treated as empty when absent (historical
/// WorkUnits stay readable; readers treat all new fields as optional).
fn validate_work_unit(body: &Value, ref_ids: &HashMap<String, String>) -> Result<(), String> {
    if body.get("schema_version").and_then(Value::as_i64) != Some(2) {
        return Err("schema_version 必须为 2".into());
    }
    let check_list = |key: &str, require_value: bool| -> Result<(), String> {
        if let Some(items) = body.get(key).and_then(Value::as_array) {
            for (i, item) in items.iter().enumerate() {
                if require_value && item.get("value").and_then(Value::as_str).unwrap_or_default().trim().is_empty()
                {
                    return Err(format!("{key}[{i}] value 不能为空"));
                }
                let refs = item.get("evidence_refs").and_then(Value::as_array);
                let Some(refs) = refs else {
                    return Err(format!("{key}[{i}] 缺少 evidence_refs"));
                };
                if refs.is_empty() {
                    return Err(format!("{key}[{i}] 没有任何证据支持"));
                }
                for r in refs {
                    let key = r.as_str().unwrap_or_default();
                    if !ref_ids.contains_key(key) {
                        return Err(format!("{key}[{i}] 引用 {key} 不在证据包中"));
                    }
                }
            }
        }
        Ok(())
    };
    for key in ["inputs", "actions", "decisions", "exceptions", "outputs"] {
        check_list(key, false)?;
    }
    for key in ["process", "environment", "details"] {
        check_list(key, true)?;
    }
    if let Some(result) = body.get("result") {
        let has_value = !result.is_null()
            && result.get("value").map(|v| !v.is_null()).unwrap_or(false);
        if has_value {
            let refs = result.get("evidence_refs").and_then(Value::as_array);
            if refs.map(|r| r.is_empty()).unwrap_or(true) {
                return Err("result 有值但无证据引用".into());
            }
        }
    }
    Ok(())
}

/// Backfill shares the extract path with a lower priority kind.
pub fn backfill_extract_handler() -> super::worker::HandlerFn {
    extract_handler()
}

#[allow(dead_code)]
fn suppressed_source_marker(_: KnowledgeSourceInput) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity_ledger::ACTIVITY_LEDGER_PRODUCER;
    use crate::knowledge::summarize::DiscoveryLimits;
    use crate::knowledge::executor::{KnowledgeModelExecutor, ModelIdentity};
    use screenpipe_config::DbConfig;
    use screenpipe_db::{ActivityEvidenceDraft, ActivityIntervalDraft, ActivityTaskDraft};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn parse_json_object_tolerates_fences() {
        assert!(parse_json_object("{\"a\":1}").is_some());
        assert!(parse_json_object("```json\n{\"a\":1}\n```").is_some());
        assert!(parse_json_object("not json").is_none());
        assert!(parse_json_object("[1,2]").is_none(), "arrays are not objects");
    }

    #[test]
    fn parse_json_object_extracts_object_from_prose() {
        let wrapped = "好的，以下是结果：\n```json\n{\"a\": {\"b\": 1}, \"s\": \"含 } 花括号\"}\n```\n希望有帮助";
        let value = parse_json_object(wrapped).expect("prose-wrapped object parses");
        assert_eq!(value["a"]["b"], 1);
        assert_eq!(value["s"], "含 } 花括号");
        assert!(parse_json_object("完全没有大括号的回答").is_none());
        assert!(parse_json_object("{未闭合的对象").is_none());
    }

    #[test]
    fn work_unit_validation_enforces_field_refs() {
        let mut refs = HashMap::new();
        refs.insert("e1".to_string(), "src-1".to_string());
        refs.insert("s1".to_string(), "42".to_string());
        let body = json!({
            "schema_version": 2,
            "task": {"title": "需求评审"},
            "actions": [{"value": "修改了方案", "evidence_refs": ["e1"]}],
            "result": {"value": null, "evidence_refs": []}
        });
        assert!(validate_work_unit(&body, &refs).is_ok());

        let unsupported = json!({
            "schema_version": 2,
            "task": {"title": "需求评审"},
            "actions": [{"value": "改了方案", "evidence_refs": []}],
            "result": null
        });
        assert!(validate_work_unit(&unsupported, &refs).is_err());

        let forged = json!({
            "schema_version": 2,
            "task": {"title": "需求评审"},
            "actions": [{"value": "改了方案", "evidence_refs": ["e9"]}],
            "result": null
        });
        assert!(validate_work_unit(&forged, &refs).is_err());

        let unsupported_result = json!({
            "schema_version": 2,
            "task": {"title": "需求评审"},
            "actions": [{"value": "改了方案", "evidence_refs": ["e1"]}],
            "result": {"value": "发布成功", "evidence_refs": []}
        });
        assert!(validate_work_unit(&unsupported_result, &refs).is_err(), "推断的 result 必须无证据即拒绝");
    }

    #[test]
    fn work_unit_schema_v2_is_required() {
        let refs = HashMap::new();
        assert!(validate_work_unit(&json!({"schema_version": 1}), &refs).is_err());
        assert!(validate_work_unit(&json!({}), &refs).is_err());
        assert!(validate_work_unit(&json!({"schema_version": 2}), &refs).is_ok());
    }

    #[test]
    fn work_unit_v2_new_fields_follow_list_rules() {
        let refs = HashMap::from([
            ("s1".to_string(), "42".to_string()),
            ("e1".to_string(), "src-1".to_string()),
        ]);
        // Absent new fields mean empty lists — nothing to reject.
        let legacy = json!({
            "schema_version": 2,
            "actions": [{"value": "修改了方案", "evidence_refs": ["s1"]}]
        });
        assert!(validate_work_unit(&legacy, &refs).is_ok());

        let valid = json!({
            "schema_version": 2,
            "process": [{"value": "先摘要后召回", "evidence_refs": ["s1"]}],
            "environment": [{"value": "macOS + cargo", "evidence_refs": ["s1"]}],
            "details": [{"value": "cargo test -p screenpipe-engine", "evidence_refs": ["e1"]}]
        });
        assert!(validate_work_unit(&valid, &refs).is_ok());

        // Non-empty value is enforced for the new fields only.
        let empty_value = json!({
            "schema_version": 2,
            "details": [{"value": "  ", "evidence_refs": ["e1"]}]
        });
        assert!(validate_work_unit(&empty_value, &refs).is_err());

        // …and so are missing or forged evidence refs.
        let no_refs = json!({"schema_version": 2, "details": [{"value": "x"}]});
        assert!(validate_work_unit(&no_refs, &refs).is_err());
        let forged_ref = json!({
            "schema_version": 2,
            "process": [{"value": "x", "evidence_refs": ["s9"]}]
        });
        assert!(validate_work_unit(&forged_ref, &refs).is_err());
    }

    #[test]
    fn sample_uniform_keeps_endpoints_and_bounds() {
        let items: Vec<usize> = (0..100).collect();
        let (picked, sampled) = sample_uniform(items, 40);
        assert!(sampled);
        assert_eq!(picked.len(), 40);
        assert_eq!(picked[0], 0);
        assert_eq!(picked[39], 99);
        // Evenly spaced: strictly increasing.
        assert!(picked.windows(2).all(|w| w[0] < w[1]));

        let small: Vec<usize> = (0..5).collect();
        let (picked, sampled) = sample_uniform(small, 40);
        assert!(!sampled);
        assert_eq!(picked.len(), 5);
    }

    #[test]
    fn parse_recall_accepts_only_in_pack_summary_ids() {
        let summary_ids = HashMap::from([("s1".to_string(), 7i64), ("s2".to_string(), 8i64)]);
        assert!(parse_recall(None, &summary_ids).unwrap().is_empty());
        assert_eq!(
            parse_recall(Some(&json!(["s2", "s1", "s2"])), &summary_ids).unwrap(),
            vec!["s2".to_string(), "s1".to_string()],
            "duplicated ids are deduped, order preserved"
        );
        // Unknown id, raw-evidence id and over-the-cap are all rejected.
        assert!(parse_recall(Some(&json!(["s9"])), &summary_ids).is_err());
        assert!(parse_recall(Some(&json!(["e1"])), &summary_ids).is_err());
        assert!(parse_recall(Some(&json!(["s1"])), &HashMap::new()).is_err());
        let six: Vec<String> = (1..=6).map(|k| format!("s{k}")).collect();
        assert!(parse_recall(Some(&json!(six)), &summary_ids).is_err());
        assert!(parse_recall(Some(&json!("s1")), &summary_ids).is_err());
    }

    async fn test_db() -> Arc<DatabaseManager> {
        Arc::new(
            DatabaseManager::new("sqlite::memory:", DbConfig::default())
                .await
                .unwrap(),
        )
    }

    fn at(value: &str) -> DateTime<Utc> {
        value.parse().unwrap()
    }

    /// Scripted executor: pops one queued response per call, records every
    /// user prompt and counts the total. Never touches the network.
    struct ScriptedExecutor {
        outputs: std::sync::Mutex<VecDeque<Result<String, KnowledgeError>>>,
        calls: AtomicUsize,
        prompts: std::sync::Mutex<Vec<String>>,
    }

    impl ScriptedExecutor {
        fn new(outputs: Vec<Result<String, KnowledgeError>>) -> Arc<Self> {
            Arc::new(Self {
                outputs: std::sync::Mutex::new(outputs.into()),
                calls: AtomicUsize::new(0),
                prompts: std::sync::Mutex::new(Vec::new()),
            })
        }

        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        fn prompts(&self) -> Vec<String> {
            self.prompts.lock().unwrap().clone()
        }
    }

    impl KnowledgeModelExecutor for ScriptedExecutor {
        fn identity(&self) -> ModelIdentity {
            ModelIdentity {
                preset_id: "fixture-preset".into(),
                provider_catalog_id: "fixture-preset".into(),
                model_id: "fixture-model".into(),
                wire_api: "openai-completions".into(),
                pi_version: "test".into(),
                profile_version: "knowledge-extract-v1".into(),
            }
        }

        fn complete(
            &self,
            request: CompletionRequest,
            _cancel: CancellationToken,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<String, KnowledgeError>> + Send + '_>,
        > {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.prompts.lock().unwrap().push(request.user_prompt);
                let mut queue = self.outputs.lock().unwrap();
                queue
                    .pop_front()
                    .unwrap_or_else(|| Err(KnowledgeError::new("script_exhausted", "无脚本输出", false)))
            })
        }
    }

    fn executor_of(scripted: &Arc<ScriptedExecutor>) -> BudgetedExecutor {
        BudgetedExecutor::new(scripted.clone(), KnowledgeLimits::MAX_MODEL_CALLS)
    }

    /// One interval with one ui_event evidence row, optionally carrying a
    /// committed interval summary citing that row. Returns the interval id.
    async fn seed_interval(
        db: &DatabaseManager,
        interval_key: &str,
        start_at: &str,
        end_at: &str,
        element_value: &str,
        summary: Option<(&str, Vec<String>)>,
    ) -> i64 {
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events \
             (timestamp, relative_ms, event_type, app_name, window_title, element_value) \
             VALUES (?1, 0, 'click', 'Arc', 'Pull request', ?2) RETURNING id",
        )
        .bind(start_at)
        .bind(element_value)
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let parent = ActivityTaskDraft {
            task_key: format!("{interval_key}-parent"),
            parent_task_key: None,
            kind: "category".into(),
            title: "Browser".into(),
            app_name: Some("Arc".into()),
            confidence: 0.8,
        };
        let task = ActivityTaskDraft {
            task_key: format!("{interval_key}-task"),
            parent_task_key: Some(format!("{interval_key}-parent")),
            kind: "task".into(),
            title: "Ticket 42".into(),
            app_name: Some("Arc".into()),
            confidence: 0.8,
        };
        let interval = ActivityIntervalDraft {
            interval_key: interval_key.to_string(),
            task_key: format!("{interval_key}-task"),
            start_at: at(start_at),
            end_at: at(end_at),
            state: "final".into(),
            confidence: 0.8,
            actions: Vec::new(),
            evidence: vec![ActivityEvidenceDraft {
                source_type: "ui_event".into(),
                source_id,
                occurred_at: at(start_at),
                action_key: None,
            }],
            retention: HashMap::from([("ui_event".to_string(), (0, None))]),
        };
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at(start_at),
            at(end_at),
            &[parent, task],
            &[interval],
        )
        .await
        .unwrap();
        let interval_id: i64 =
            sqlx::query_scalar("SELECT id FROM activity_intervals WHERE interval_key = ?1")
                .bind(interval_key)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        if let Some((summary, keywords)) = summary {
            let refs = json!([{"source_type": "ui_event", "source_id": source_id}]).to_string();
            let keywords_json = serde_json::to_string(&keywords).unwrap();
            let chars = summary.chars().filter(|c| !c.is_whitespace()).count() as i64;
            sqlx::query(
                "INSERT INTO activity_interval_summaries \
                 (interval_id, summary, keywords, band, summary_chars, evidence_refs, producer, prompt_version, model, input_hash) \
                 VALUES (?1, ?2, ?3, 'short', ?4, ?5, 'summarizer-v1', 'zh-summary-v1', 'fixture-model', ?6)",
            )
            .bind(interval_id)
            .bind(summary)
            .bind(&keywords_json)
            .bind(chars)
            .bind(&refs)
            .bind(format!("{interval_key}-summary-hash"))
            .execute(&db.pool)
            .await
            .unwrap();
        }
        interval_id
    }

    /// One final interval with no evidence rows at all (settle grace
    /// bypassed by past timestamps).
    async fn seed_interval_without_evidence(
        db: &DatabaseManager,
        interval_key: &str,
        start_at: &str,
        end_at: &str,
    ) -> i64 {
        let parent = ActivityTaskDraft {
            task_key: format!("{interval_key}-parent"),
            parent_task_key: None,
            kind: "category".into(),
            title: "Browser".into(),
            app_name: Some("Arc".into()),
            confidence: 0.8,
        };
        let task = ActivityTaskDraft {
            task_key: format!("{interval_key}-task"),
            parent_task_key: Some(format!("{interval_key}-parent")),
            kind: "task".into(),
            title: "Empty".into(),
            app_name: Some("Arc".into()),
            confidence: 0.8,
        };
        let interval = ActivityIntervalDraft {
            interval_key: interval_key.to_string(),
            task_key: format!("{interval_key}-task"),
            start_at: at(start_at),
            end_at: at(end_at),
            state: "final".into(),
            confidence: 0.8,
            actions: Vec::new(),
            evidence: Vec::new(),
            retention: HashMap::new(),
        };
        db.reconcile_activity_ledger(
            ACTIVITY_LEDGER_PRODUCER,
            at(start_at),
            at(end_at),
            &[parent, task],
            &[interval],
        )
        .await
        .unwrap();
        sqlx::query_scalar("SELECT id FROM activity_intervals WHERE interval_key = ?1")
            .bind(interval_key)
            .fetch_one(&db.pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn discovery_prescreens_unsettled_and_evidence_free_intervals() {
        let db = test_db().await;
        // One boundary-complete rebuild of the same producer over the whole
        // scan window, carrying all three variants:
        // - eligible: settled + evidence (the only eligible target)
        // - hollow: settled but evidence-free (§4.1.4 pre-screen)
        // - fresh: evidence but still inside the settle grace
        let start = Utc::now() - chrono::Duration::minutes(20);
        let end = Utc::now() - chrono::Duration::minutes(1);
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events \
             (timestamp, relative_ms, event_type, app_name, window_title, element_value) \
             VALUES (?1, 0, 'click', 'Arc', 'Pull request', 'Clicked Reply on Ticket 42') \
             RETURNING id",
        )
        .bind(start.to_rfc3339())
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let parent = ActivityTaskDraft {
            task_key: "multi-parent".into(),
            parent_task_key: None,
            kind: "category".into(),
            title: "Browser".into(),
            app_name: Some("Arc".into()),
            confidence: 0.8,
        };
        let task = ActivityTaskDraft {
            task_key: "multi-task".into(),
            parent_task_key: Some("multi-parent".into()),
            kind: "task".into(),
            title: "Ticket 42".into(),
            app_name: Some("Arc".into()),
            confidence: 0.8,
        };
        let evidence = vec![ActivityEvidenceDraft {
            source_type: "ui_event".into(),
            source_id,
            occurred_at: start,
            action_key: None,
        }];
        let mut eligible = ActivityIntervalDraft {
            interval_key: "eligible".into(),
            task_key: "multi-task".into(),
            start_at: start,
            end_at: Utc::now() - chrono::Duration::minutes(10),
            state: "final".into(),
            confidence: 0.8,
            actions: Vec::new(),
            evidence: evidence.clone(),
            retention: HashMap::from([("ui_event".to_string(), (0, None))]),
        };
        let hollow = ActivityIntervalDraft {
            interval_key: "hollow".into(),
            task_key: "multi-task".into(),
            start_at: Utc::now() - chrono::Duration::minutes(9),
            end_at: Utc::now() - chrono::Duration::minutes(8),
            state: "final".into(),
            confidence: 0.8,
            actions: Vec::new(),
            evidence: Vec::new(),
            retention: HashMap::new(),
        };
        let fresh = ActivityIntervalDraft {
            interval_key: "fresh".into(),
            task_key: "multi-task".into(),
            start_at: Utc::now() - chrono::Duration::minutes(4),
            end_at: Utc::now() - chrono::Duration::minutes(1),
            state: "final".into(),
            confidence: 0.8,
            actions: Vec::new(),
            evidence: evidence.clone(),
            retention: HashMap::from([("ui_event".to_string(), (0, None))]),
        };
        db.reconcile_activity_ledger(
            ACTIVITY_LEDGER_PRODUCER,
            start,
            end,
            &[parent, task],
            &[eligible, hollow, fresh],
        )
        .await
        .unwrap();

        let created = discover_and_enqueue(&db, Utc::now() - chrono::Duration::hours(26), DiscoveryLimits::default())
            .await
            .unwrap();
        assert_eq!(
            created, 1,
            "only the settled, evidence-bearing interval gets a job"
        );
        let jobs: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT kind, scope_key FROM knowledge_jobs")
                .fetch_all(&db.pool)
                .await
                .unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].0, "extract");
    }

    #[tokio::test]
    async fn new_activity_in_the_window_reenqueues_extraction() {
        let db = test_db().await;
        seed_interval(
            &db,
            "solo",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            "在终端运行 cargo test 验证改动",
            None,
        )
        .await;
        let created = discover_and_enqueue(&db, at("2026-08-17T09:00:00Z"), DiscoveryLimits::default())
            .await
            .unwrap();
        assert_eq!(created, 1);

        // Drive the job to a terminal state: identical inputs must stay
        // blocked (§4.1.1) even though discovery keeps seeing the interval.
        let claimed = db
            .knowledge_claim_next_job(&[KnowledgeJobKind::Extract], "w", 30_000)
            .await
            .unwrap()
            .unwrap();
        db.knowledge_complete_job(claimed.id, &claimed.lease_token, None, None)
            .await
            .unwrap();
        let created = discover_and_enqueue(&db, at("2026-08-17T09:00:00Z"), DiscoveryLimits::default())
            .await
            .unwrap();
        assert_eq!(
            created, 0,
            "a succeeded job blocks re-enqueueing for identical inputs"
        );

        // A new activity lands inside the same window: the member set changes
        // → the hash changes → extraction is allowed again (§4.1.2), and the
        // newcomer gets its own job for its own (narrower) window. The
        // rebuild covers the whole window boundary-completely.
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let late_source: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events \
             (timestamp, relative_ms, event_type, app_name, window_title, element_value) \
             VALUES ('2026-08-17T09:01:30Z', 0, 'text', 'Arc', 'Pull request', '补充了新的证据内容') \
             RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let late_evidence = vec![ActivityEvidenceDraft {
            source_type: "ui_event".into(),
            source_id: late_source,
            occurred_at: at("2026-08-17T09:01:30Z"),
            action_key: None,
        }];
        let solo_task = ActivityTaskDraft {
            task_key: "solo-task".into(),
            parent_task_key: Some("solo-parent".into()),
            kind: "task".into(),
            title: "Ticket 42".into(),
            app_name: Some("Arc".into()),
            confidence: 0.8,
        };
        let solo_parent = ActivityTaskDraft {
            task_key: "solo-parent".into(),
            parent_task_key: None,
            kind: "category".into(),
            title: "Browser".into(),
            app_name: Some("Arc".into()),
            confidence: 0.8,
        };
        let mut solo = ActivityIntervalDraft {
            interval_key: "solo".into(),
            task_key: "solo-task".into(),
            start_at: at("2026-08-17T09:00:00Z"),
            end_at: at("2026-08-17T09:05:00Z"),
            state: "final".into(),
            confidence: 0.8,
            actions: Vec::new(),
            evidence: Vec::new(),
            retention: HashMap::new(),
        };
        // Keep solo's original evidence so its member identity is stable.
        let solo_id: i64 =
            sqlx::query_scalar("SELECT id FROM activity_intervals WHERE interval_key = 'solo'")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        let (orig_source, orig_at): (i64, String) = sqlx::query_as(
            "SELECT source_id, occurred_at FROM activity_evidence WHERE interval_id = ?1",
        )
        .bind(solo_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        solo.evidence = vec![ActivityEvidenceDraft {
            source_type: "ui_event".into(),
            source_id: orig_source,
            occurred_at: DateTime::parse_from_rfc3339(&orig_at)
                .map(|t| t.with_timezone(&Utc))
                .unwrap(),
            action_key: None,
        }];
        let late = ActivityIntervalDraft {
            interval_key: "late-arrival".into(),
            task_key: "solo-task".into(),
            start_at: at("2026-08-17T09:01:00Z"),
            end_at: at("2026-08-17T09:02:00Z"),
            state: "final".into(),
            confidence: 0.8,
            actions: Vec::new(),
            evidence: late_evidence,
            retention: HashMap::from([("ui_event".to_string(), (0, None))]),
        };
        db.reconcile_activity_ledger(
            ACTIVITY_LEDGER_PRODUCER,
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:05:00Z"),
            &[solo_parent, solo_task],
            &[solo, late],
        )
        .await
        .unwrap();
        let created = discover_and_enqueue(&db, at("2026-08-17T09:00:00Z"), DiscoveryLimits::default())
            .await
            .unwrap();
        // With strict [start, end) discovery, the original boundary-only
        // observation is intentionally excluded; only the genuinely new
        // activity is enqueued. This still protects new activity -> new hash
        // -> permitted recomputation.
        assert_eq!(created, 1, "only the newcomer crosses the half-open window");
    }

    /// §4.1.4 attempt-3 counterexample: a job queued against the v1 ledger
    /// version must NOT run the model after a v2 rebuild replaced the window
    /// (the window still has active intervals — the OLD exists-check passed).
    /// The queued discovery hash must match the current active inputs; the
    /// replacement job runs normally.
    #[tokio::test]
    async fn stale_version_job_skips_the_model_and_new_version_runs() {
        let db = test_db().await;
        let (start, end) = ("2026-08-17T09:00:00Z", "2026-08-17T09:05:00Z");
        seed_interval(
            &db,
            "v1-interval",
            start,
            end,
            "在终端运行 cargo test 验证改动",
            None,
        )
        .await;
        let created = discover_and_enqueue(&db, at(start), DiscoveryLimits::default())
            .await
            .unwrap();
        assert_eq!(created, 1);
        let h1: String = sqlx::query_scalar(
            "SELECT input_hash FROM knowledge_jobs ORDER BY id LIMIT 1",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();

        // Rebuild the same window under v2: a new interval id becomes the
        // active version.
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let v2_source: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events \
             (timestamp, relative_ms, event_type, app_name, window_title, element_value) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'text', 'Arc', 'Pull request', 'v2 重建后的新证据') \
             RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let v2_parent = ActivityTaskDraft {
            task_key: "v2-parent".into(),
            parent_task_key: None,
            kind: "category".into(),
            title: "Browser".into(),
            app_name: Some("Arc".into()),
            confidence: 0.8,
        };
        let v2_task = ActivityTaskDraft {
            task_key: "v2-task".into(),
            parent_task_key: Some("v2-parent".into()),
            kind: "task".into(),
            title: "Ticket 42".into(),
            app_name: Some("Arc".into()),
            confidence: 0.8,
        };
        let v2_interval = ActivityIntervalDraft {
            interval_key: "v2-interval".into(),
            task_key: "v2-task".into(),
            start_at: at(start),
            end_at: at(end),
            state: "final".into(),
            confidence: 0.8,
            actions: Vec::new(),
            evidence: vec![ActivityEvidenceDraft {
                source_type: "ui_event".into(),
                source_id: v2_source,
                occurred_at: at("2026-08-17T09:01:00Z"),
                action_key: None,
            }],
            retention: HashMap::from([("ui_event".to_string(), (0, None))]),
        };
        db.reconcile_activity_ledger(
            "deterministic-v2",
            at(start),
            at(end),
            &[v2_parent, v2_task],
            &[v2_interval],
        )
        .await
        .unwrap();

        // The replacement discovery enqueues a NEW hash for the new version.
        let created = discover_and_enqueue(&db, at(start), DiscoveryLimits::default())
            .await
            .unwrap();
        assert_eq!(created, 1, "the active v2 interval is discovered");
        let h2: String = sqlx::query_scalar(
            "SELECT input_hash FROM knowledge_jobs ORDER BY id DESC LIMIT 1",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_ne!(h1, h2, "the version change must change the input hash");

        let scope = "Arc|ticket 42";
        // The STALE job: hash mismatch → refused before any model call.
        let stale_executor = ScriptedExecutor::new(vec![]);
        let failure = extract_interval(
            &db,
            &executor_of(&stale_executor),
            CancellationToken::new(),
            scope,
            start,
            end,
            Some(&h1),
        )
        .await
        .unwrap_err();
        assert_eq!(failure.code, "superseded_input");
        assert!(!failure.transient);
        assert_eq!(stale_executor.call_count(), 0, "zero model calls on stale input");

        // The replacement job runs normally with exactly one call.
        let fresh_executor = ScriptedExecutor::new(vec![Ok(json!({
            "work_unit": {"schema_version": 2, "task": {"title": "v2 提取"}},
            "recall": [],
        })
        .to_string())]);
        let committed = extract_interval(
            &db,
            &executor_of(&fresh_executor),
            CancellationToken::new(),
            scope,
            start,
            end,
            Some(&h2),
        )
        .await
        .unwrap();
        assert_eq!(fresh_executor.call_count(), 1);
        assert!(!committed.unit_id.is_empty());
    }

    #[test]
    fn discovery_input_hash_tracks_member_activities() {
        let scope = "arc|ticket 42";
        let a = vec![("interval-a".to_string(), "2026-08-17T09:00:00Z".into(), "2026-08-17T09:05:00Z".into())];
        let same_members_reordered = vec![a[0].clone()];
        let with_newcomer = vec![
            a[0].clone(),
            ("interval-b".to_string(), "2026-08-17T09:01:00Z".into(), "2026-08-17T09:02:00Z".into()),
        ];
        assert_eq!(
            discovery_input_hash(scope, &a),
            discovery_input_hash(scope, &same_members_reordered)
        );
        assert_ne!(
            discovery_input_hash(scope, &a),
            discovery_input_hash(scope, &with_newcomer),
            "a new activity in the window must change the hash"
        );
        assert_ne!(
            discovery_input_hash(scope, &a),
            discovery_input_hash("arc|other", &a),
            "scope participates in the hash"
        );
    }

    #[tokio::test]
    async fn summary_pack_lists_summaries_and_cites_evidence() {
        let db = test_db().await;
        seed_interval(
            &db,
            "pack-interval",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            "把 screenpipe 仓库的 B03a 计划提交到 review",
            Some((
                "在 screenpipe 仓库推进 B03a：把 WorkUnit 提取切到摘要优先输入。",
                vec!["screenpipe".into(), "B03a".into()],
            )),
        )
        .await;
        let pack = build_summary_pack(&db, "2026-08-17T09:00:00Z", "2026-08-17T09:05:00Z")
            .await
            .unwrap();
        assert!(pack.prompt_text.contains("[s1] "));
        assert!(pack.prompt_text.contains(" ~ 2026-08-17T09:05:00+00:00｜Arc｜Ticket 42"));
        assert!(pack.prompt_text.contains("摘要：在 screenpipe 仓库推进 B03a"));
        assert!(pack.prompt_text.contains("关键字：screenpipe、B03a"));
        assert!(pack.prompt_text.contains("采样记账：ui_event: 保留 1，丢弃 0"));
        assert!(pack.prompt_text.contains("[e1] (ui_event #"));
        assert!(pack.summary_ids.contains_key("s1"));
        // Every row is registered: identity + dependencies stay on raw sources.
        assert_eq!(pack.sources.len(), 1);
        assert!(pack.ref_ids.contains_key("e1"));
        assert!(pack.ref_ids.contains_key("s1"));
    }

    #[tokio::test]
    async fn summary_pack_falls_back_to_raw_evidence_without_summaries() {
        let db = test_db().await;
        seed_interval(
            &db,
            "raw-interval",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            "Clicked Reply on Ticket 42",
            None,
        )
        .await;
        let pack = build_summary_pack(&db, "2026-08-17T09:00:00Z", "2026-08-17T09:05:00Z")
            .await
            .unwrap();
        assert!(pack.summary_ids.is_empty(), "no summaries → nothing recallable");
        assert!(pack.prompt_text.contains("[原始证据]"), "degrades to the raw pack");
        assert!(pack.prompt_text.contains("[e1] (ui_event) Clicked Reply on Ticket 42"));
        assert!(!pack.prompt_text.contains("[s1]"));
        assert_eq!(pack.sources.len(), 1);
    }

    #[tokio::test]
    async fn summary_pack_samples_when_window_exceeds_caps() {
        let db = test_db().await;
        let summary_text = "在screenpipe仓库排查活动台账的索引缺失，并修复了保留记账的统计口径。".repeat(20);
        for k in 0..45 {
            let minute = format!("{:02}", k);
            let start = format!("2026-08-17T09:{minute}:00Z");
            let end = format!("2026-08-17T09:{minute}:30Z");
            seed_interval(
                &db,
                &format!("cap-{k}"),
                &start,
                &end,
                "排查保留记账",
                Some((&summary_text, vec!["screenpipe".into()])),
            )
            .await;
        }
        let pack = build_summary_pack(&db, "2026-08-17T09:00:00Z", "2026-08-17T09:46:00Z")
            .await
            .unwrap();
        let count = pack.summary_ids.len();
        assert!(count <= MAX_SUMMARY_INTERVALS, "interval cap binds: {count}");
        assert!(count >= 1);
        assert!(pack.prompt_text.contains("均匀取样"), "sampling must be noted");
        assert!(
            pack.prompt_text.len() <= MAX_PACK_CHARS,
            "total pack must stay bounded: {} bytes",
            pack.prompt_text.len()
        );
        // Renumbered contiguously after sampling: s1..sK, time-ordered endpoints.
        let ids: HashSet<&String> = pack.summary_ids.keys().collect();
        assert_eq!(ids.len(), count);
        assert!(pack.summary_ids.contains_key("s1"));
    }

    #[tokio::test]
    async fn two_phase_recall_runs_second_stage_and_commits_final_body() {
        let db = test_db().await;
        seed_interval(
            &db,
            "recall-interval",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            "在终端运行 cargo test -p screenpipe-engine 验证 B03a 改动",
            Some((
                "在 screenpipe 仓库推进 B03a：把 WorkUnit 提取切到摘要优先，并回捞原始证据核实了验证命令。",
                vec!["screenpipe".into(), "B03a".into()],
            )),
        )
        .await;
        let scripted = ScriptedExecutor::new(vec![
            // Phase 1: names the summary block for recall.
            Ok(json!({
                "work_unit": {"schema_version": 2, "task": {"title": "草稿"}},
                "recall": ["s1"],
            })
            .to_string()),
            // Phase 2: final WorkUnit citing both the summary and recalled raw evidence.
            Ok(json!({
                "schema_version": 2,
                "task": {"title": "B03a WorkUnit 提取改造", "app": "Arc"},
                "actions": [{"value": "切换到摘要优先输入", "evidence_refs": ["s1"]}],
                "details": [{"value": "cargo test -p screenpipe-engine", "evidence_refs": ["e1"]}],
                "result": null
            })
            .to_string()),
        ]);
        let committed = extract_interval(
            &db,
            &executor_of(&scripted),
            CancellationToken::new(),
            "arc|ticket 42",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            None,
        )
        .await
        .unwrap();
        assert_eq!(scripted.call_count(), 2, "phase 1 + phase 2, no repair");

        let prompts = scripted.prompts();
        assert!(prompts[0].contains("[s1]"), "phase 1 carries the summary pack");
        assert!(prompts[0].contains(PHASE_ONE_RECALL_NOTE));
        assert!(
            prompts[1].contains("[s1] 的原始证据"),
            "phase 2 carries the recalled interval"
        );
        assert!(
            prompts[1].contains("[e1] (ui_event) 在终端运行 cargo test"),
            "recalled rows keep their eN labels"
        );
        assert!(!prompts[1].contains(PHASE_ONE_RECALL_NOTE));

        let (body, prompt_version): (String, String) = sqlx::query_as(
            "SELECT body, prompt_version FROM knowledge_work_unit_revisions WHERE input_hash = ?1",
        )
        .bind(&committed.input_hash)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(prompt_version, "zh-extract-v3");
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["schema_version"], 2);
        assert!(parsed.get("recall").is_none(), "final body must not carry recall");
        assert_eq!(parsed["details"][0]["value"], "cargo test -p screenpipe-engine");
        // Dependencies stay registered on the raw evidence source.
        let deps: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM knowledge_dependencies WHERE consumer_kind = 'work_unit' AND consumer_id = ?1",
        )
        .bind(&committed.unit_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(deps, 1);
    }

    #[tokio::test]
    async fn empty_recall_costs_exactly_one_call() {
        let db = test_db().await;
        seed_interval(
            &db,
            "short-interval",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            "Clicked Reply on Ticket 42",
            Some(("回复了工单 42 的评审意见。", vec!["工单".into()])),
        )
        .await;
        let work_unit = json!({
            "schema_version": 2,
            "task": {"title": "回复工单"},
            "actions": [{"value": "回复评审意见", "evidence_refs": ["s1"]}],
            "result": null
        });
        let scripted = ScriptedExecutor::new(vec![Ok(json!({
            "work_unit": work_unit,
            "recall": [],
        })
        .to_string())]);
        let committed = extract_interval(
            &db,
            &executor_of(&scripted),
            CancellationToken::new(),
            "arc|ticket 42",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            None,
        )
        .await
        .unwrap();
        assert_eq!(scripted.call_count(), 1, "empty recall must not trigger phase 2");
        let body: String = sqlx::query_scalar(
            "SELECT body FROM knowledge_work_unit_revisions WHERE input_hash = ?1",
        )
        .bind(&committed.input_hash)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["schema_version"], 2);
        assert_eq!(parsed["actions"][0]["value"], "回复评审意见");
    }

    #[tokio::test]
    async fn illegal_recall_is_rejected_without_a_second_call() {
        let db = test_db().await;
        seed_interval(
            &db,
            "illegal-interval",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            "Clicked Reply on Ticket 42",
            Some(("回复了工单 42 的评审意见。", vec!["工单".into()])),
        )
        .await;
        for bad_recall in ["s9", "e1"] {
            let scripted = ScriptedExecutor::new(vec![Ok(json!({
                "work_unit": {"schema_version": 2, "task": {"title": "x"}},
                "recall": [bad_recall],
            })
            .to_string())]);
            let failure = extract_interval(
                &db,
                &executor_of(&scripted),
                CancellationToken::new(),
                "arc|ticket 42",
                "2026-08-17T09:00:00Z",
                "2026-08-17T09:05:00Z",
                None,
            )
            .await
            .unwrap_err();
            assert_eq!(failure.code, format!("invalid_recall:{bad_recall} 不是包内间隔编号"));
            assert!(!failure.transient, "fabricated ids are permanent, not retryable");
            assert_eq!(scripted.call_count(), 1, "no phase 2 on fabricated ids");
        }
        let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_work_units").fetch_one(&db.pool).await.unwrap();
        assert_eq!(stored, 0, "rejected output must not be written");
    }

    #[tokio::test]
    async fn recall_pack_is_bounded_per_interval_and_total() {
        let db = test_db().await;
        // Five summarized intervals with long evidence text: the per-interval
        // budget (2000) and the total budget (8000 = 4 × 2000) both bind.
        let long_text = "在终端运行 cargo test 并核对输出结果。".repeat(300);
        for k in 0..5 {
            let minute = format!("2026-08-17T09:0{k}:00Z");
            let end = format!("2026-08-17T09:0{k}:30Z");
            seed_interval(
                &db,
                &format!("bounded-{k}"),
                &minute,
                &end,
                &long_text,
                Some(("推进 B03a 的间隔摘要。", vec!["B03a".into()])),
            )
            .await;
        }
        let pack = build_summary_pack(&db, "2026-08-17T09:00:00Z", "2026-08-17T09:04:30Z")
            .await
            .unwrap();
        let recall_ids: Vec<String> = (1..=5).map(|k| format!("s{k}")).collect();
        let recall = build_recall_pack(&pack, &recall_ids);
        // Per-interval budget: the first recalled section stays near 2000 chars.
        let s1 = recall.split("[s2] 的原始证据").next().unwrap();
        let body = s1.split("以上是召回的原始证据").next().unwrap();
        assert!(
            body.chars().count() <= RECALL_INTERVAL_CHARS + 64,
            "per-interval recall must stay near 2000 chars, got {}",
            body.chars().count()
        );
        assert!(recall.contains("[s1] 的原始证据"));
        assert!(recall.contains("[s4] 的原始证据"));
        assert!(
            !recall.contains("[s5] 的原始证据"),
            "total budget exhausted before the fifth interval"
        );
        assert!(recall.contains("已截断"), "budget overflow must be noted");
    }

    #[test]
    fn nomination_is_optional_and_shape_safe() {
        assert_eq!(parse_nomination(&json!({"work_unit": {}})), None, "缺字段 = 未提名");
        assert_eq!(
            parse_nomination(&json!({"knowledge_nomination": {"nominated": "yes"}})),
            None,
            "形状不对 = 未提名，绝不阻断抽取"
        );
        assert_eq!(
            parse_nomination(&json!({"knowledge_nomination": {"nominated": false}})),
            Some(KnowledgeNomination { nominated: false, flow: None }),
        );
        assert_eq!(
            parse_nomination(&json!({"knowledge_nomination": {"nominated": true, "flow": " 工单处理 "}})),
            Some(KnowledgeNomination { nominated: true, flow: Some("工单处理".into()) }),
        );
        assert_eq!(
            parse_nomination(&json!({"knowledge_nomination": {"nominated": true, "flow": "   "}})),
            Some(KnowledgeNomination { nominated: true, flow: None }),
            "空 flow 归一为 None"
        );
    }

    #[test]
    fn strip_nomination_keeps_the_body_clean_for_compile_prompts() {
        let body = json!({
            "schema_version": 2,
            "knowledge_nomination": {"nominated": true, "flow": "x"},
            "actions": []
        });
        let stripped = strip_nomination(body);
        assert!(stripped.get("knowledge_nomination").is_none());
        assert!(stripped.get("actions").is_some());
    }

    /// ④ 机制一 + 解耦验收：成功抽取记录提名、正文不带提名、并且绝不即时
    /// 入队 compile——投递只归 ④ tick 的三机制管（plan §4.2.2）。
    #[tokio::test]
    async fn extract_records_nomination_and_never_enqueues_compile() {
        let db = test_db().await;
        seed_interval(
            &db,
            "nominate-interval",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            "在终端运行 cargo test 验证改动",
            Some(("回复工单评审并跑验证命令。", vec!["工单".into()])),
        )
        .await;
        let scripted = ScriptedExecutor::new(vec![Ok(json!({
            "work_unit": {
                "schema_version": 2,
                "task": {"title": "回复工单"},
                "actions": [{"value": "回复评审意见", "evidence_refs": ["s1"]}],
                "knowledge_nomination": {"nominated": true, "flow": "工单处理"}
            },
            "recall": [],
            "knowledge_nomination": {"nominated": true, "flow": "工单处理"}
        })
        .to_string())]);
        let committed = extract_interval(
            &db,
            &executor_of(&scripted),
            CancellationToken::new(),
            "arc|ticket 42",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            None,
        )
        .await
        .unwrap();
        assert!(committed.nominated);

        // 提名落库：scope + unit + 建议流程。
        let (flow,): (Option<String>,) = sqlx::query_as(
            "SELECT flow FROM knowledge_distill_nominations WHERE scope_key = ?1 AND work_unit_id = ?2",
        )
        .bind("arc|ticket 42")
        .bind(&committed.unit_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(flow.as_deref(), Some("工单处理"));

        // 正文不带提名（compile 提示词不携带投递记账）。
        let (body,): (String,) = sqlx::query_as(
            "SELECT body FROM knowledge_work_unit_revisions WHERE input_hash = ?1",
        )
        .bind(&committed.input_hash)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert!(!body.contains("knowledge_nomination"));

        // 移除即时 compile 入队：抽取成功后 0 条 compile 任务。
        let compile_jobs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM knowledge_jobs WHERE kind = 'compile'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(compile_jobs, 0, "compile 只能由 ④ tick 三机制投递");
    }

    #[tokio::test]
    async fn unnamed_extraction_stays_unnominated() {
        let db = test_db().await;
        seed_interval(
            &db,
            "plain-interval",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            "Clicked Reply on Ticket 42",
            Some(("回复了工单 42 的评审意见。", vec!["工单".into()])),
        )
        .await;
        // 输出没有任何提名字段：不得记提名，也不得投递。
        let scripted = ScriptedExecutor::new(vec![Ok(json!({
            "work_unit": {"schema_version": 2, "task": {"title": "回复工单"}},
            "recall": [],
        })
        .to_string())]);
        let committed = extract_interval(
            &db,
            &executor_of(&scripted),
            CancellationToken::new(),
            "arc|ticket 42",
            "2026-08-17T09:00:00Z",
            "2026-08-17T09:05:00Z",
            None,
        )
        .await
        .unwrap();
        assert!(!committed.nominated);
        let nominations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_distill_nominations")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(nominations, 0, "未提名不落库");
    }

    /// §4.2：`knowledgeDiscoveryBatch` 是每轮发现投递上限。
    #[tokio::test]
    async fn discovery_batch_caps_extractions_per_round() {
        let db = test_db().await;
        for k in 0..3 {
            seed_interval(
                &db,
                &format!("batch-{k}"),
                &format!("2026-08-17T09:0{k}:00Z"),
                &format!("2026-08-17T09:0{k}:30Z"),
                "排查保留记账",
                None,
            )
            .await;
        }
        let created = discover_and_enqueue(
            &db,
            at("2026-08-17T09:00:00Z"),
            DiscoveryLimits { batch: 2, ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(created, 2, "本轮最多投 2 个");
        let created = discover_and_enqueue(
            &db,
            at("2026-08-17T09:00:00Z"),
            DiscoveryLimits { batch: 2, ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(created, 1, "下一轮补齐剩余候选");
    }
}
