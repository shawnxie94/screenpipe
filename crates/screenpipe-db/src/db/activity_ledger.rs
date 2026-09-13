// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

use super::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityLedgerObservation {
    pub source_type: String,
    pub source_id: i64,
    pub occurred_at: DateTime<Utc>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub browser_url: Option<String>,
    pub document_path: Option<String>,
    pub semantic_kind: Option<String>,
    pub semantic_title: Option<String>,
    pub semantic_key: Option<String>,
    pub event_type: Option<String>,
    pub click_count: Option<i64>,
    pub element_role: Option<String>,
    pub element_name: Option<String>,
    pub device_name: Option<String>,
    pub speaker_name: Option<String>,
    pub is_input_device: Option<bool>,
    pub attention_rank: i64,
    /// Content/state fingerprint used by the retention policy's change
    /// detection: `frames.content_hash` for frames (falling back to a
    /// lightweight text fingerprint computed in SQL when the hash is NULL),
    /// the `(event_type, element_name, element_value, text_content)` tuple for
    /// UI events, `None` for audio.
    pub content_fingerprint: Option<String>,
    /// The observation carries no retainable content at all — an empty
    /// transcript, or a frame with no content hash and no text. Marked
    /// truthfully by the loader; such candidates are dropped with reason
    /// `empty` instead of being pre-filtered away.
    pub content_empty: bool,
}

#[derive(Debug, Clone)]
pub struct ActivityTaskDraft {
    pub task_key: String,
    pub parent_task_key: Option<String>,
    pub kind: String,
    pub title: String,
    pub app_name: Option<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct ActivityEvidenceDraft {
    pub source_type: String,
    pub source_id: i64,
    pub occurred_at: DateTime<Utc>,
    pub action_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActivityActionDraft {
    pub action_key: String,
    pub occurred_at: DateTime<Utc>,
    pub action_type: String,
    pub summary: String,
    pub app_name: Option<String>,
    pub confidence: f64,
    pub source_type: String,
    pub source_id: i64,
}

#[derive(Debug, Clone)]
pub struct ActivityIntervalDraft {
    pub interval_key: String,
    pub task_key: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub state: String,
    pub confidence: f64,
    pub actions: Vec<ActivityActionDraft>,
    pub evidence: Vec<ActivityEvidenceDraft>,
    /// Retention bookkeeping per source type: `source_type -> (dropped, drop_reason)`.
    /// `kept` is derived from the evidence rows persisted in the same
    /// transaction, so the draft only reports what was considered and dropped.
    pub retention: HashMap<String, (i64, Option<String>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvidenceRecord {
    pub source_type: String,
    pub source_id: i64,
    pub occurred_at: String,
    pub frame_id: Option<i64>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub browser_url: Option<String>,
}

/// One evidence citation stored with an interval summary: a pointer back to
/// a retained `activity_evidence` row of the same interval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivitySummaryEvidenceRef {
    pub source_type: String,
    pub source_id: i64,
}

/// A stored interval summary row (B02a producer read face).
#[derive(Debug, Clone)]
pub struct ActivitySummaryRow {
    pub interval_id: i64,
    pub summary: String,
    pub keywords: Vec<String>,
    pub band: String,
    pub summary_chars: i64,
    pub evidence_refs: Vec<ActivitySummaryEvidenceRef>,
    pub producer: String,
    pub prompt_version: String,
    pub model: Option<String>,
    pub input_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityActionRecord {
    pub id: i64,
    pub occurred_at: String,
    pub action_type: String,
    pub summary: String,
    pub app_name: Option<String>,
    pub confidence: f64,
    pub source_type: String,
    pub source_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityIntervalRecord {
    pub id: i64,
    pub task_id: i64,
    pub parent_task_id: Option<i64>,
    pub kind: String,
    pub title: String,
    pub parent_title: Option<String>,
    pub app_name: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub state: String,
    pub confidence: f64,
    pub producer: String,
    pub evidence_count: i64,
    pub actions: Vec<ActivityActionRecord>,
    pub evidence: Vec<ActivityEvidenceRecord>,
    /// Persisted interval summary, if the summarizer has produced one.
    pub summary: Option<String>,
    /// Keyword list stored as a JSON array next to the summary.
    pub keywords: Option<Vec<String>>,
    /// Summary length band (`short` | `medium` | `long`).
    pub summary_band: Option<String>,
    /// Per-source retention accounting: `(source_type, kept, dropped, drop_reason)`.
    pub retention: Vec<(String, i64, i64, Option<String>)>,
}

#[derive(FromRow)]
struct RawObservation {
    source_type: String,
    source_id: i64,
    /// Raw RFC3339 text from the source table, parsed in Rust. SQLite's date
    /// functions are millisecond-only, and rounding a microsecond frame or
    /// ui-event timestamp can carry an event that happened just before an
    /// interval boundary onto the boundary itself — breaking the half-open
    /// `[start_at, end_at)` evidence contract. Unparseable text falls back to
    /// `DateTime::UNIX_EPOCH`, which the loader's retain filter drops.
    occurred_at_text: String,
    app_name: Option<String>,
    window_title: Option<String>,
    browser_url: Option<String>,
    document_path: Option<String>,
    semantic_kind: Option<String>,
    semantic_title: Option<String>,
    semantic_key: Option<String>,
    event_type: Option<String>,
    click_count: Option<i64>,
    element_role: Option<String>,
    element_name: Option<String>,
    device_name: Option<String>,
    speaker_name: Option<String>,
    is_input_device: Option<bool>,
    attention_rank: i64,
    content_fingerprint: Option<String>,
    content_empty: bool,
}

#[derive(FromRow)]
struct RawInterval {
    id: i64,
    task_id: i64,
    parent_task_id: Option<i64>,
    kind: String,
    title: String,
    parent_title: Option<String>,
    app_name: Option<String>,
    start_at: String,
    end_at: String,
    state: String,
    confidence: f64,
    producer: String,
    evidence_count: i64,
}

#[derive(FromRow)]
struct RawAction {
    id: i64,
    interval_id: i64,
    occurred_at: String,
    action_type: String,
    summary: String,
    app_name: Option<String>,
    confidence: f64,
    source_type: String,
    source_id: i64,
}

#[derive(FromRow)]
struct RawEvidence {
    interval_id: i64,
    source_type: String,
    source_id: i64,
    occurred_at: String,
    frame_id: Option<i64>,
    app_name: Option<String>,
    window_title: Option<String>,
    browser_url: Option<String>,
}

/// Interval row joined with its summary (both nullable) for the retention-era
/// read models; `into_record` normalizes the keyword JSON and attaches
/// per-source retention counts.
#[derive(FromRow)]
struct RawIntervalDetail {
    id: i64,
    task_id: i64,
    parent_task_id: Option<i64>,
    kind: String,
    title: String,
    parent_title: Option<String>,
    app_name: Option<String>,
    start_at: String,
    end_at: String,
    state: String,
    confidence: f64,
    producer: String,
    evidence_count: i64,
    summary: Option<String>,
    keywords: Option<String>,
    band: Option<String>,
}

impl RawIntervalDetail {
    fn into_record(
        self,
        retention: Vec<(String, i64, i64, Option<String>)>,
    ) -> ActivityIntervalRecord {
        let keywords = self
            .keywords
            .and_then(|keywords| serde_json::from_str::<Vec<String>>(&keywords).ok());
        ActivityIntervalRecord {
            id: self.id,
            task_id: self.task_id,
            parent_task_id: self.parent_task_id,
            kind: self.kind,
            title: self.title,
            parent_title: self.parent_title,
            app_name: self.app_name,
            start_at: self.start_at,
            end_at: self.end_at,
            state: self.state,
            confidence: self.confidence,
            producer: self.producer,
            evidence_count: self.evidence_count,
            actions: Vec::new(),
            evidence: Vec::new(),
            summary: self.summary,
            keywords,
            summary_band: self.band,
            retention,
        }
    }
}

impl From<RawObservation> for ActivityLedgerObservation {
    fn from(row: RawObservation) -> Self {
        Self {
            source_type: row.source_type,
            source_id: row.source_id,
            occurred_at: DateTime::parse_from_rfc3339(&row.occurred_at_text)
                .map(|value| value.with_timezone(&Utc))
                .unwrap_or(DateTime::UNIX_EPOCH),
            app_name: nonempty(row.app_name),
            window_title: nonempty(row.window_title),
            browser_url: nonempty(row.browser_url),
            document_path: nonempty(row.document_path),
            semantic_kind: nonempty(row.semantic_kind),
            semantic_title: nonempty(row.semantic_title),
            semantic_key: nonempty(row.semantic_key),
            event_type: nonempty(row.event_type),
            click_count: row.click_count,
            element_role: nonempty(row.element_role),
            element_name: nonempty(row.element_name),
            device_name: nonempty(row.device_name),
            speaker_name: nonempty(row.speaker_name),
            is_input_device: row.is_input_device,
            attention_rank: row.attention_rank,
            content_fingerprint: row.content_fingerprint,
            content_empty: row.content_empty,
        }
    }
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

// Active-version rule: every default read (list, stats, summary discovery,
// WorkUnit evidence packs) goes through the `activity_intervals_active` view
// created by 20260912102000_activity_ledger_active_versions.sql — the single
// SQL source of truth for §4.4.4. Only deliberate by-id history reads bypass
// it.

/// Boundary-complete switch bounds for a coverage switch over
/// `[start_text, end_text)` (§4.4.4, attempt 3): widen to the fixpoint of
/// every interval that either is active-visible or belongs to the target
/// producer and straddles an edge. Returns the final bounds and whether they
/// grew. Runs on the caller's connection so the read happens inside the same
/// transaction as the switch — no concurrent drift between expansion and
/// commit. Non-convergence after 8 rounds is an error, never a partial
/// switch.
async fn expand_switch_bounds_tx(
    conn: &mut sqlx::SqliteConnection,
    target_producer: &str,
    start_text: &str,
    end_text: &str,
) -> Result<(String, String, bool), SqlxError> {
    let mut start = start_text.to_string();
    let mut end = end_text.to_string();
    let mut widened = false;
    for _ in 0..8 {
        let visible: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            r#"SELECT MIN(i.start_at), MAX(i.end_at)
               FROM activity_intervals_active i
               WHERE i.start_at < ?2 AND i.end_at > ?1
                 AND (i.start_at < ?1 OR i.end_at > ?2)"#,
        )
        .bind(&start)
        .bind(&end)
        .fetch_optional(&mut *conn)
        .await?;
        let target: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            r#"SELECT MIN(start_at), MAX(end_at)
               FROM activity_intervals
               WHERE producer = ?3
                 AND start_at < ?2 AND end_at > ?1
                 AND (start_at < ?1 OR end_at > ?2)"#,
        )
        .bind(&start)
        .bind(&end)
        .bind(target_producer)
        .fetch_optional(&mut *conn)
        .await?;
        let before = (start.clone(), end.clone());
        for (min_start, max_end) in [visible, target].into_iter().flatten() {
            if let Some(value) = min_start {
                if value.as_str() < start.as_str() {
                    start = value;
                }
            }
            if let Some(value) = max_end {
                if value.as_str() > end.as_str() {
                    end = value;
                }
            }
        }
        if start == before.0 && end == before.1 {
            return Ok((start, end, widened));
        }
        widened = true;
    }
    Err(SqlxError::Protocol(
        "activity ledger coverage switch did not converge to boundary-complete bounds".to_string(),
    ))
}

impl DatabaseManager {
    /// Load a bounded, metadata-only observation stream. Frames are sampled to
    /// one stable context row per ten seconds; raw AX JSON and transcript text
    /// never enter this query or get duplicated into the ledger.
    pub async fn load_activity_ledger_observations(
        &self,
        start_at: DateTime<Utc>,
        end_at: DateTime<Utc>,
    ) -> Result<Vec<ActivityLedgerObservation>, SqlxError> {
        let start = start_at.to_rfc3339();
        let end = end_at.to_rfc3339();

        let frames = sqlx::query_as::<_, RawObservation>(
            r#"WITH sampled AS (
                   SELECT MIN(id) AS id
                   FROM frames
                   WHERE timestamp >= ?1
                     AND timestamp < ?2
                     AND COALESCE(focused, 1) = 1
                     AND (COALESCE(app_name, '') != '' OR COALESCE(window_name, '') != '')
                   GROUP BY
                     CAST(((julianday(timestamp) - 2440587.5) * 86400.0) / 10 AS INTEGER),
                     COALESCE(app_name, ''), COALESCE(window_name, ''),
                     COALESCE(browser_url, ''), COALESCE(document_path, '')
               )
               SELECT 'frame' AS source_type, f.id AS source_id,
                      f.timestamp AS occurred_at_text,
                      f.app_name, f.window_name AS window_title, f.browser_url,
                      f.document_path,
                      (SELECT si.kind
                         FROM semantic_run_items sri
                         JOIN semantic_items si ON si.id = sri.item_id
                        WHERE sri.run_id = f.semantic_run_id
                          AND COALESCE(si.title, '') != ''
                        ORDER BY CASE si.kind
                            WHEN 'task' THEN 0 WHEN 'conversation' THEN 1
                            WHEN 'document' THEN 2 WHEN 'page' THEN 3 ELSE 4 END,
                            sri.sort_order LIMIT 1) AS semantic_kind,
                      (SELECT si.title
                         FROM semantic_run_items sri
                         JOIN semantic_items si ON si.id = sri.item_id
                        WHERE sri.run_id = f.semantic_run_id
                          AND COALESCE(si.title, '') != ''
                        ORDER BY CASE si.kind
                            WHEN 'task' THEN 0 WHEN 'conversation' THEN 1
                            WHEN 'document' THEN 2 WHEN 'page' THEN 3 ELSE 4 END,
                            sri.sort_order LIMIT 1) AS semantic_title,
                      (SELECT si.item_key
                         FROM semantic_run_items sri
                         JOIN semantic_items si ON si.id = sri.item_id
                        WHERE sri.run_id = f.semantic_run_id
                          AND COALESCE(si.title, '') != ''
                        ORDER BY CASE si.kind
                            WHEN 'task' THEN 0 WHEN 'conversation' THEN 1
                            WHEN 'document' THEN 2 WHEN 'page' THEN 3 ELSE 4 END,
                            sri.sort_order LIMIT 1) AS semantic_key,
                      NULL AS event_type, NULL AS click_count,
                      NULL AS element_role, NULL AS element_name,
                      NULL AS device_name, NULL AS speaker_name, NULL AS is_input_device,
                      CASE WHEN f.focused = 1 THEN 3 ELSE 2 END AS attention_rank,
                      -- Retention fingerprint: prefer the capture-time content
                      -- hash; when it is NULL fall back to a lightweight text
                      -- fingerprint (length + head/tail bytes) so the raw text
                      -- never has to enter this metadata query.
                      CASE
                        WHEN f.content_hash IS NOT NULL
                          THEN CAST(f.content_hash AS TEXT)
                        WHEN COALESCE(f.full_text, '') != ''
                          OR COALESCE(f.accessibility_text, '') != ''
                          THEN 't' || COALESCE(length(f.full_text), 0) || ':' ||
                               hex(substr(COALESCE(f.full_text, ''), 1, 32)) || ':' ||
                               hex(substr(COALESCE(f.accessibility_text, ''), 1, 32)) || ':' ||
                               hex(substr(COALESCE(f.full_text, ''), -32)) || ':' ||
                               hex(substr(COALESCE(f.accessibility_text, ''), -32))
                        ELSE NULL
                      END AS content_fingerprint,
                      CASE
                        WHEN f.content_hash IS NULL
                             AND COALESCE(f.full_text, '') = ''
                             AND COALESCE(f.accessibility_text, '') = ''
                          THEN 1 ELSE 0
                      END AS content_empty
               FROM sampled s JOIN frames f ON f.id = s.id"#,
        )
        .bind(&start)
        .bind(&end)
        .fetch_all(&self.pool);

        let ui_events = sqlx::query_as::<_, RawObservation>(
            r#"SELECT 'ui_event' AS source_type, u.id AS source_id,
                      u.timestamp AS occurred_at_text,
                      COALESCE(NULLIF(u.app_name, ''), f.app_name) AS app_name,
                      COALESCE(NULLIF(u.window_title, ''), f.window_name) AS window_title,
                      COALESCE(NULLIF(u.browser_url, ''), f.browser_url) AS browser_url,
                      f.document_path,
                      NULL AS semantic_kind, NULL AS semantic_title, NULL AS semantic_key,
                      u.event_type, u.click_count, u.element_role, u.element_name,
                      NULL AS device_name, NULL AS speaker_name, NULL AS is_input_device,
                      4 AS attention_rank,
                      u.event_type || '|' || COALESCE(u.element_name, '') || '|' ||
                        COALESCE(u.element_value, '') || '|' ||
                        COALESCE(u.text_content, '') AS content_fingerprint,
                      0 AS content_empty
               FROM ui_events u
               LEFT JOIN frames f ON f.id = u.frame_id
               WHERE u.timestamp >= ?1
                 AND u.timestamp < ?2
                 AND u.event_type IN ('click', 'text', 'clipboard', 'app_switch', 'window_focus')
               ORDER BY u.timestamp, u.id"#,
        )
        .bind(&start)
        .bind(&end)
        .fetch_all(&self.pool);

        let audio = sqlx::query_as::<_, RawObservation>(
            r#"SELECT 'audio' AS source_type, a.id AS source_id,
                      a.timestamp AS occurred_at_text,
                      NULL AS app_name, NULL AS window_title, NULL AS browser_url,
                      NULL AS document_path, NULL AS semantic_kind,
                      NULL AS semantic_title, NULL AS semantic_key, NULL AS event_type,
                      NULL AS click_count,
                      NULL AS element_role, NULL AS element_name,
                      a.device AS device_name, s.name AS speaker_name,
                      a.is_input_device, 1 AS attention_rank,
                      NULL AS content_fingerprint,
                      -- Empty transcripts flow through so the retention policy
                      -- can drop and account for them itself (plan §4.2).
                      CASE WHEN length(trim(a.transcription)) = 0
                           THEN 1 ELSE 0 END AS content_empty
               FROM audio_transcriptions a
               LEFT JOIN speakers s ON s.id = a.speaker_id
               WHERE a.timestamp >= ?1
                 AND a.timestamp < ?2
               ORDER BY a.timestamp, a.id"#,
        )
        .bind(&start)
        .bind(&end)
        .fetch_all(&self.pool);

        let (frames, ui_events, audio) = tokio::try_join!(frames, ui_events, audio)?;
        let mut observations = Vec::with_capacity(frames.len() + ui_events.len() + audio.len());
        observations.extend(frames.into_iter().map(Into::into));
        observations.extend(ui_events.into_iter().map(Into::into));
        observations.extend(audio.into_iter().map(Into::into));
        observations.retain(|row: &ActivityLedgerObservation| {
            row.occurred_at != DateTime::UNIX_EPOCH
                && row.occurred_at >= start_at
                && row.occurred_at < end_at
        });
        observations.sort_by(|left, right| {
            left.occurred_at
                .cmp(&right.occurred_at)
                .then(left.attention_rank.cmp(&right.attention_rank))
                .then(left.source_type.cmp(&right.source_type))
                .then(left.source_id.cmp(&right.source_id))
        });
        Ok(observations)
    }

    pub async fn activity_ledger_last_reconciled_at(
        &self,
        producer: &str,
    ) -> Result<Option<DateTime<Utc>>, SqlxError> {
        let value: Option<String> = sqlx::query_scalar(
            "SELECT last_reconciled_at FROM activity_ledger_state WHERE producer = ?1",
        )
        .bind(producer)
        .fetch_optional(&self.pool)
        .await?;
        Ok(value.and_then(|value| {
            DateTime::parse_from_rfc3339(&value)
                .ok()
                .map(|value| value.with_timezone(&Utc))
        }))
    }

    /// Replace one producer's trailing interpretation in a single coordinated
    /// write. Model/network work must finish before this method is called.
    pub async fn reconcile_activity_ledger(
        &self,
        producer: &str,
        range_start: DateTime<Utc>,
        range_end: DateTime<Utc>,
        tasks: &[ActivityTaskDraft],
        intervals: &[ActivityIntervalDraft],
    ) -> Result<(), SqlxError> {
        let range_start_text = range_start.to_rfc3339();
        let range_end_text = range_end.to_rfc3339();
        let mut tx = self.begin_immediate_with_retry().await?;

        // §4.4.4 boundary guard (attempt 3), inside the same transaction as
        // the switch: a rebuild range that bisects any active or same-producer
        // segment must NOT switch coverage over a partial span. Callers widen
        // via `activity_ledger_expand_range_to_active_bounds` and retry.
        let (switch_start, switch_end, widened) = expand_switch_bounds_tx(
            &mut **tx.conn(),
            producer,
            &range_start_text,
            &range_end_text,
        )
        .await?;
        if widened {
            tx.rollback().await?;
            return Err(SqlxError::Protocol(format!(
                "activity ledger rebuild range is not boundary-complete; \
                 rebuild over [{switch_start},{switch_end}) instead"
            )));
        }

        // Use the transactionally widened bounds for every delete, prefix
        // lookup, and coverage write below. Mixing the caller's narrow bounds
        // with the boundary-complete switch bounds leaves stale edge rows and
        // makes a second identical reconcile derive different identities.
        let switch_start_text = switch_start;
        let switch_end_text = switch_end;
        let switch_start_at = DateTime::parse_from_rfc3339(&switch_start_text)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|error| SqlxError::Protocol(format!("bad switch bound: {error}")))?;
        let interval_keys: Vec<&str> = intervals
            .iter()
            .filter(|interval| interval.end_at > interval.start_at)
            .map(|interval| interval.interval_key.as_str())
            .collect();

        // Preserve the finalized prefix of a segment crossing the reconciliation
        // watermark, then replace everything beginning inside the trailing range.
        sqlx::query(
            "DELETE FROM activity_actions WHERE interval_id IN (\
                 SELECT id FROM activity_intervals \
                 WHERE producer = ?1 AND start_at >= ?2 AND start_at < ?3)",
        )
        .bind(producer)
        .bind(&switch_start_text)
        .bind(&switch_end_text)
        .execute(&mut **tx.conn())
        .await?;
        // Evidence deletion is intentionally deferred. The AFTER DELETE
        // trigger removes an interval when its last evidence disappears;
        // deleting before keyed upsert therefore destroys the very row whose
        // id the rerun must preserve. Upserts below refresh matching evidence.
        sqlx::query(
            "DELETE FROM activity_actions WHERE interval_id IN (\
                 SELECT id FROM activity_intervals \
                 WHERE producer = ?1 AND start_at < ?2 \
                   AND end_at > ?2) \
             AND occurred_at >= ?2",
        )
        .bind(producer)
        .bind(&switch_start_text)
        .execute(&mut **tx.conn())
        .await?;
        sqlx::query(
            "UPDATE activity_intervals SET end_at = ?2, state = 'final', \
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE producer = ?1 AND start_at < ?2 \
               AND end_at > ?2",
        )
        .bind(producer)
        .bind(&switch_start_text)
        .execute(&mut **tx.conn())
        .await?;
        let mut task_ids = HashMap::with_capacity(tasks.len());
        for task in tasks.iter().filter(|task| task.parent_task_key.is_none()) {
            upsert_task(&mut tx, producer, task, None).await?;
            let id = task_id(&mut tx, &task.task_key).await?;
            task_ids.insert(task.task_key.clone(), id);
        }
        for task in tasks.iter().filter(|task| task.parent_task_key.is_some()) {
            let parent_id = task
                .parent_task_key
                .as_ref()
                .and_then(|key| task_ids.get(key))
                .copied();
            upsert_task(&mut tx, producer, task, parent_id).await?;
            let id = task_id(&mut tx, &task.task_key).await?;
            task_ids.insert(task.task_key.clone(), id);
        }

        let mut first_interval = true;
        for interval in intervals {
            if interval.end_at <= interval.start_at {
                continue;
            }
            let Some(task_id) = task_ids.get(&interval.task_key).copied() else {
                return Err(SqlxError::Protocol(format!(
                    "activity interval references missing task {}",
                    interval.task_key
                )));
            };
            // A moving reconciliation watermark must not leave one tiny
            // interval every poll. If the first rebuilt segment continues the
            // same task within one frame-sampling period, extend the preserved
            // prefix instead of inserting a new segment.
            let merge_prefix = first_interval
                && interval.start_at >= switch_start_at
                && interval.start_at - switch_start_at <= chrono::Duration::seconds(15);
            // Prefer the deterministic interval key on reruns. The prefix
            // extension is only a fallback for a genuinely new first segment;
            // otherwise it can steal a stable keyed row and change the visible
            // id even though the rebuilt content is identical.
            let keyed_id = sqlx::query_scalar::<_, i64>(
                "SELECT id FROM activity_intervals WHERE interval_key = ?1 LIMIT 1",
            )
            .bind(&interval.interval_key)
            .fetch_optional(&mut **tx.conn())
            .await?;
            let keyed_id = match keyed_id {
                Some(id) => Some(id),
                None => {
                    sqlx::query_scalar::<_, i64>(
                        "SELECT id FROM activity_intervals \
                     WHERE producer = ?1 AND start_at = ?2 \
                     ORDER BY id LIMIT 1",
                    )
                    .bind(producer)
                    .bind(interval.start_at.to_rfc3339())
                    .fetch_optional(&mut **tx.conn())
                    .await?
                }
            };
            let prefix_id = keyed_id.or(if merge_prefix {
                sqlx::query_scalar::<_, i64>(
                    "SELECT id FROM activity_intervals \
                     WHERE producer = ?1 AND task_id = ?2 \
                       AND end_at = ?3 \
                       AND start_at < ?3 \
                     ORDER BY start_at DESC, id DESC LIMIT 1",
                )
                .bind(producer)
                .bind(task_id)
                .bind(&switch_start_text)
                .fetch_optional(&mut **tx.conn())
                .await?
            } else {
                None
            });
            first_interval = false;
            let interval_id: i64 = if let Some(prefix_id) = prefix_id {
                sqlx::query(
                    "UPDATE activity_intervals SET end_at = ?1, state = ?2, confidence = ?3, \
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
                     WHERE id = ?4",
                )
                .bind(interval.end_at.to_rfc3339())
                .bind(&interval.state)
                .bind(interval.confidence.clamp(0.0, 1.0))
                .bind(prefix_id)
                .execute(&mut **tx.conn())
                .await?;
                prefix_id
            } else {
                sqlx::query_scalar(
                    r#"INSERT INTO activity_intervals
                       (interval_key, task_id, start_at, end_at, state, confidence, producer)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                       ON CONFLICT(interval_key) DO UPDATE SET
                         task_id = excluded.task_id, start_at = excluded.start_at,
                         end_at = excluded.end_at, state = excluded.state,
                         confidence = excluded.confidence, producer = excluded.producer,
                         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       RETURNING id"#,
                )
                .bind(&interval.interval_key)
                .bind(task_id)
                .bind(interval.start_at.to_rfc3339())
                .bind(interval.end_at.to_rfc3339())
                .bind(&interval.state)
                .bind(interval.confidence.clamp(0.0, 1.0))
                .bind(producer)
                .fetch_one(&mut **tx.conn())
                .await?
            };

            let mut action_ids = HashMap::with_capacity(interval.actions.len());
            for action in &interval.actions {
                let action_id: i64 = sqlx::query_scalar(
                    r#"INSERT INTO activity_actions
                       (action_key, interval_id, occurred_at, action_type, summary,
                        app_name, confidence, source_type, source_id)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                       ON CONFLICT(action_key) DO UPDATE SET
                         interval_id = excluded.interval_id,
                         occurred_at = excluded.occurred_at,
                         action_type = excluded.action_type,
                         summary = excluded.summary,
                         app_name = excluded.app_name,
                         confidence = excluded.confidence,
                         source_type = excluded.source_type,
                         source_id = excluded.source_id
                       RETURNING id"#,
                )
                .bind(&action.action_key)
                .bind(interval_id)
                .bind(action.occurred_at.to_rfc3339())
                .bind(&action.action_type)
                .bind(&action.summary)
                .bind(&action.app_name)
                .bind(action.confidence.clamp(0.0, 1.0))
                .bind(&action.source_type)
                .bind(action.source_id)
                .fetch_one(&mut **tx.conn())
                .await?;
                action_ids.insert(action.action_key.clone(), action_id);
            }

            for evidence in &interval.evidence {
                let action_id = evidence
                    .action_key
                    .as_ref()
                    .and_then(|key| action_ids.get(key))
                    .copied();
                sqlx::query(
                    r#"INSERT INTO activity_evidence
                       (interval_id, action_id, source_type, source_id, occurred_at)
                       VALUES (?1, ?2, ?3, ?4, ?5)
                       ON CONFLICT(interval_id, source_type, source_id) DO UPDATE SET
                         action_id = excluded.action_id,
                         occurred_at = excluded.occurred_at"#,
                )
                .bind(interval_id)
                .bind(action_id)
                .bind(&evidence.source_type)
                .bind(evidence.source_id)
                .bind(evidence.occurred_at.to_rfc3339())
                .execute(&mut **tx.conn())
                .await?;
            }

            // Retention accounting lands in the same transaction as the
            // evidence it describes. `kept` counts the rows actually persisted
            // for the interval — after a prefix merge that includes the
            // preserved pre-range evidence — so reruns converge on the same
            // numbers. Rewriting the interval's rows keeps stale sources from
            // a previous policy lingering.
            sqlx::query("DELETE FROM activity_interval_retention WHERE interval_id = ?1")
                .bind(interval_id)
                .execute(&mut **tx.conn())
                .await?;
            for (source_type, (dropped, drop_reason)) in &interval.retention {
                let kept: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM activity_evidence \
                     WHERE interval_id = ?1 AND source_type = ?2",
                )
                .bind(interval_id)
                .bind(source_type)
                .fetch_one(&mut **tx.conn())
                .await?;
                sqlx::query(
                    r#"INSERT INTO activity_interval_retention
                       (interval_id, source_type, kept, dropped, drop_reason)
                       VALUES (?1, ?2, ?3, ?4, ?5)
                       ON CONFLICT(interval_id, source_type) DO UPDATE SET
                         kept = excluded.kept,
                         dropped = excluded.dropped,
                         drop_reason = excluded.drop_reason,
                         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
                )
                .bind(interval_id)
                .bind(source_type)
                .bind(kept)
                .bind(dropped)
                .bind(drop_reason)
                .execute(&mut **tx.conn())
                .await?;
            }
        }

        // Remove only obsolete keyed rows after all replacements have a chance
        // to resolve their existing interval identity.
        let existing: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, interval_key FROM activity_intervals \
             WHERE producer = ?1 AND start_at >= ?2 AND start_at < ?3",
        )
        .bind(producer)
        .bind(&switch_start_text)
        .bind(&switch_end_text)
        .fetch_all(&mut **tx.conn())
        .await?;
        for (id, key) in existing {
            if !interval_keys.iter().any(|expected| *expected == key) {
                sqlx::query("DELETE FROM activity_intervals WHERE id = ?1")
                    .bind(id)
                    .execute(&mut **tx.conn())
                    .await?;
            }
        }

        sqlx::query(
            r#"INSERT INTO activity_ledger_state (producer, last_reconciled_at)
               VALUES (?1, ?2)
               ON CONFLICT(producer) DO UPDATE SET
                 last_reconciled_at = excluded.last_reconciled_at,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(producer)
        .bind(&switch_end_text)
        .execute(&mut **tx.conn())
        .await?;

        // Active-version switch (§4.4.4) commits in the SAME transaction as
        // the rebuilt segments: a failed reconcile leaves the previous
        // coverage — and therefore the previous active version — untouched.
        // Same-producer rows fully inside the new range are superseded by the
        // fresh row (keeps the tick from growing the table without bound);
        // rows from other producers and any partially overlapping history are
        // kept so explicit rollback and by-id history stay available.
        sqlx::query(
            "DELETE FROM activity_ledger_coverage \
             WHERE producer = ?1 AND range_start >= ?2 AND range_end <= ?3",
        )
        .bind(producer)
        .bind(&switch_start_text)
        .bind(&switch_end_text)
        .execute(&mut **tx.conn())
        .await?;
        sqlx::query(
            "INSERT INTO activity_ledger_coverage (producer, range_start, range_end) \
             VALUES (?1, ?2, ?3)",
        )
        .bind(producer)
        .bind(&switch_start_text)
        .bind(&switch_end_text)
        .execute(&mut **tx.conn())
        .await?;

        cleanup_orphaned_tasks(&mut tx).await?;
        tx.commit().await
    }

    /// Expand a rebuild range to boundary-complete switch bounds (§4.4.4):
    /// the fixpoint of widening to every interval that either is currently
    /// active-visible or belongs to `producer` and straddles an edge, so a
    /// coverage switch can never leave one of their segments half-covered.
    /// Errors when the fixpoint does not converge — no partial result.
    pub async fn activity_ledger_expand_range_to_active_bounds(
        &self,
        start_at: DateTime<Utc>,
        end_at: DateTime<Utc>,
        producer: &str,
    ) -> Result<(DateTime<Utc>, DateTime<Utc>), SqlxError> {
        let mut conn = self.pool.acquire().await?;
        let (start, end, _widened) = expand_switch_bounds_tx(
            &mut conn,
            producer,
            &start_at.to_rfc3339(),
            &end_at.to_rfc3339(),
        )
        .await?;
        let parse = |value: &str| {
            DateTime::parse_from_rfc3339(value)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|error| {
                    SqlxError::Protocol(format!("bad coverage bound {value}: {error}"))
                })
        };
        Ok((parse(&start)?, parse(&end)?))
    }

    /// Whether one interval is active-visible under the coverage rule.
    /// Knowledge handlers re-check queued targets right before any model call
    /// (§4.1.4): a version superseded after enqueueing must not spend model
    /// budget.
    pub async fn activity_interval_is_active(&self, interval_id: i64) -> Result<bool, SqlxError> {
        let active: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM activity_intervals_active WHERE id = ?1")
                .bind(interval_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(active.is_some())
    }

    /// Whether the window still holds at least one active interval — the
    /// extract handler's execution-time recheck for superseded jobs.
    pub async fn activity_window_has_active_intervals(
        &self,
        start_at: DateTime<Utc>,
        end_at: DateTime<Utc>,
    ) -> Result<bool, SqlxError> {
        let start = start_at.to_rfc3339();
        let end = end_at.to_rfc3339();
        let rows: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM activity_intervals_active
               WHERE start_at >= ?1 AND start_at < ?2"#,
        )
        .bind(&start)
        .bind(&end)
        .fetch_one(&self.pool)
        .await?;
        Ok(rows > 0)
    }

    /// Explicit version switch (§4.4.4 rollback): makes `producer` the newest
    /// authority for the requested range. The switch runs on
    /// boundary-complete bounds (widened to the fixpoint of active and
    /// target-producer straddlers, inside this transaction) and is refused
    /// when the target producer's retained segments do not actually cover the
    /// range — never activates into a hole. Recency is the coverage row id;
    /// version strings are never compared lexically.
    pub async fn activity_ledger_activate_version(
        &self,
        producer: &str,
        range_start: DateTime<Utc>,
        range_end: DateTime<Utc>,
    ) -> Result<(), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let (start_text, end_text, _widened) = expand_switch_bounds_tx(
            &mut **tx.conn(),
            producer,
            &range_start.to_rfc3339(),
            &range_end.to_rfc3339(),
        )
        .await?;
        // Target coverage completeness: the retained segments of `producer`
        // must tile [start_text, end_text) without gaps.
        let segments: Vec<(String, String)> = sqlx::query_as(
            "SELECT start_at, end_at FROM activity_intervals \
             WHERE producer = ?1 AND end_at > ?2 AND start_at < ?3 \
             ORDER BY start_at, end_at",
        )
        .bind(producer)
        .bind(&start_text)
        .bind(&end_text)
        .fetch_all(&mut **tx.conn())
        .await?;
        let parse = |value: &str| {
            DateTime::parse_from_rfc3339(value)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|error| {
                    SqlxError::Protocol(format!("bad interval bound {value}: {error}"))
                })
        };
        let mut cursor = parse(&start_text)?;
        let switch_end = parse(&end_text)?;
        for (start_at, end_at) in segments {
            let segment_start = parse(&start_at)?;
            let segment_end = parse(&end_at)?;
            if segment_start > cursor {
                return Err(SqlxError::Protocol(format!(
                    "activity ledger coverage switch refused: producer {producer} \
                     does not cover the requested range (gap before {start_at})"
                )));
            }
            if segment_end > cursor {
                cursor = segment_end;
            }
        }
        if cursor < switch_end {
            return Err(SqlxError::Protocol(format!(
                "activity ledger coverage switch refused: producer {producer} \
                 does not cover the requested range (ends at {cursor})"
            )));
        }
        sqlx::query(
            "INSERT INTO activity_ledger_coverage (producer, range_start, range_end) \
             VALUES (?1, ?2, ?3)",
        )
        .bind(producer)
        .bind(&start_text)
        .bind(&end_text)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await
    }

    /// History exception: one interval by id regardless of active coverage.
    /// Default lists/statistics/summaries use the active view; explicit
    /// by-id lookups must keep superseded versions readable.
    pub async fn activity_interval_record_by_id(
        &self,
        interval_id: i64,
    ) -> Result<Option<ActivityIntervalRecord>, SqlxError> {
        let row: Option<RawInterval> = sqlx::query_as(
            r#"SELECT i.id, t.id AS task_id, t.parent_task_id, t.kind, t.title,
                      parent.title AS parent_title, t.app_name,
                      i.start_at, i.end_at, i.state, i.confidence, i.producer,
                      (SELECT COUNT(*) FROM activity_evidence e
                        WHERE e.interval_id = i.id) AS evidence_count
               FROM activity_intervals i
               JOIN activity_tasks t ON t.id = i.task_id
               LEFT JOIN activity_tasks parent ON parent.id = t.parent_task_id
               WHERE i.id = ?1"#,
        )
        .bind(interval_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| ActivityIntervalRecord {
            id: row.id,
            task_id: row.task_id,
            parent_task_id: row.parent_task_id,
            kind: row.kind,
            title: row.title,
            parent_title: row.parent_title,
            app_name: row.app_name,
            start_at: row.start_at,
            end_at: row.end_at,
            state: row.state,
            confidence: row.confidence,
            producer: row.producer,
            evidence_count: row.evidence_count,
            actions: Vec::new(),
            evidence: Vec::new(),
            summary: None,
            keywords: None,
            summary_band: None,
            retention: Vec::new(),
        }))
    }

    pub async fn list_activity_ledger(
        &self,
        start_at: DateTime<Utc>,
        end_at: DateTime<Utc>,
        include_actions: bool,
        include_evidence: bool,
    ) -> Result<Vec<ActivityIntervalRecord>, SqlxError> {
        let start = start_at.to_rfc3339();
        let end = end_at.to_rfc3339();
        let rows = sqlx::query_as::<_, RawInterval>(
            r#"SELECT i.id, t.id AS task_id, t.parent_task_id, t.kind, t.title,
                      parent.title AS parent_title, t.app_name,
                      i.start_at, i.end_at, i.state, i.confidence, i.producer,
                      (SELECT COUNT(*) FROM activity_evidence e
                        WHERE e.interval_id = i.id) AS evidence_count
               FROM activity_intervals_active i
               JOIN activity_tasks t ON t.id = i.task_id
               LEFT JOIN activity_tasks parent ON parent.id = t.parent_task_id
               WHERE i.end_at > ?1
                 AND i.start_at < ?2
               ORDER BY i.start_at, i.end_at, i.id"#,
        )
        .bind(&start)
        .bind(&end)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let mut actions_by_interval: HashMap<i64, Vec<ActivityActionRecord>> = HashMap::new();
        let mut evidence_by_interval: HashMap<i64, Vec<ActivityEvidenceRecord>> = HashMap::new();
        if include_actions {
            let actions = sqlx::query_as::<_, RawAction>(
                r#"SELECT a.id, a.interval_id, a.occurred_at, a.action_type,
                          a.summary, a.app_name, a.confidence, a.source_type, a.source_id
                   FROM activity_actions a
                   JOIN activity_intervals i ON i.id = a.interval_id
                   WHERE i.end_at > ?1
                     AND i.start_at < ?2
                   ORDER BY a.occurred_at, a.id"#,
            )
            .bind(&start)
            .bind(&end)
            .fetch_all(&self.pool)
            .await?;
            for action in actions {
                actions_by_interval
                    .entry(action.interval_id)
                    .or_default()
                    .push(ActivityActionRecord {
                        id: action.id,
                        occurred_at: action.occurred_at,
                        action_type: action.action_type,
                        summary: action.summary,
                        app_name: action.app_name,
                        confidence: action.confidence,
                        source_type: action.source_type,
                        source_id: action.source_id,
                    });
            }
        }
        if include_evidence {
            let evidence = sqlx::query_as::<_, RawEvidence>(
                r#"SELECT e.interval_id, e.source_type, e.source_id, e.occurred_at,
                          CASE
                            WHEN e.source_type = 'frame' THEN source_frame.id
                            WHEN e.source_type = 'ui_event' THEN event_frame.id
                            ELSE NULL
                          END AS frame_id,
                          COALESCE(NULLIF(event.app_name, ''),
                                   NULLIF(source_frame.app_name, ''),
                                   NULLIF(event_frame.app_name, '')) AS app_name,
                          COALESCE(NULLIF(event.window_title, ''),
                                   NULLIF(source_frame.window_name, ''),
                                   NULLIF(event_frame.window_name, '')) AS window_title,
                          COALESCE(NULLIF(event.browser_url, ''),
                                   NULLIF(source_frame.browser_url, ''),
                                   NULLIF(event_frame.browser_url, '')) AS browser_url
                   FROM activity_evidence e
                   JOIN activity_intervals i ON i.id = e.interval_id
                   LEFT JOIN frames source_frame
                     ON e.source_type = 'frame' AND source_frame.id = e.source_id
                   LEFT JOIN ui_events event
                     ON e.source_type = 'ui_event' AND event.id = e.source_id
                   LEFT JOIN frames event_frame ON event_frame.id = event.frame_id
                   WHERE i.end_at > ?1
                     AND i.start_at < ?2
                   ORDER BY e.occurred_at, e.id"#,
            )
            .bind(&start)
            .bind(&end)
            .fetch_all(&self.pool)
            .await?;
            for item in evidence {
                evidence_by_interval
                    .entry(item.interval_id)
                    .or_default()
                    .push(ActivityEvidenceRecord {
                        source_type: item.source_type,
                        source_id: item.source_id,
                        occurred_at: item.occurred_at,
                        frame_id: item.frame_id,
                        app_name: item.app_name,
                        window_title: item.window_title,
                        browser_url: item.browser_url,
                    });
            }
        }

        Ok(rows
            .into_iter()
            .map(|row| ActivityIntervalRecord {
                id: row.id,
                task_id: row.task_id,
                parent_task_id: row.parent_task_id,
                kind: row.kind,
                title: row.title,
                parent_title: row.parent_title,
                app_name: row.app_name,
                start_at: row.start_at,
                end_at: row.end_at,
                state: row.state,
                confidence: row.confidence,
                producer: row.producer,
                evidence_count: row.evidence_count,
                actions: actions_by_interval.remove(&row.id).unwrap_or_default(),
                evidence: evidence_by_interval.remove(&row.id).unwrap_or_default(),
                summary: None,
                keywords: None,
                summary_band: None,
                retention: Vec::new(),
            })
            .collect())
    }

    /// Meeting time spans overlapping `[start_at, end_at)`, clamped to the
    /// range. An open meeting (NULL `meeting_end`) counts through `end_at`.
    /// The ledger carries no meeting signal of its own, so the retention
    /// policy decides meeting exemption purely from these spans.
    pub async fn activity_meeting_spans(
        &self,
        start_at: DateTime<Utc>,
        end_at: DateTime<Utc>,
    ) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>, SqlxError> {
        let start = start_at.to_rfc3339();
        let end = end_at.to_rfc3339();
        let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT meeting_start, meeting_end FROM meetings \
             WHERE meeting_start < ?2 AND COALESCE(meeting_end, ?2) > ?1",
        )
        .bind(&start)
        .bind(&end)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(meeting_start, meeting_end)| {
                let start = DateTime::parse_from_rfc3339(&meeting_start?)
                    .ok()?
                    .with_timezone(&Utc);
                let end = match meeting_end {
                    Some(end) => DateTime::parse_from_rfc3339(&end).ok()?.with_timezone(&Utc),
                    None => end_at,
                };
                Some((start.max(start_at), end.min(end_at)))
            })
            .filter(|(start, end)| end > start)
            .collect())
    }

    /// Read model for the activity layer: intervals with task identity,
    /// persisted summary and keywords when present, and per-source
    /// retention/drop counts. Read-only; feeds B02/B03 and the skills.
    pub async fn activity_intervals_between(
        &self,
        start_at: DateTime<Utc>,
        end_at: DateTime<Utc>,
    ) -> Result<Vec<ActivityIntervalRecord>, SqlxError> {
        let start = start_at.to_rfc3339();
        let end = end_at.to_rfc3339();
        let rows = sqlx::query_as::<_, RawIntervalDetail>(
            r#"SELECT i.id, t.id AS task_id, t.parent_task_id, t.kind, t.title,
                      parent.title AS parent_title, t.app_name,
                      i.start_at, i.end_at, i.state, i.confidence, i.producer,
                      (SELECT COUNT(*) FROM activity_evidence e
                        WHERE e.interval_id = i.id) AS evidence_count,
                      s.summary, s.keywords, s.band
               FROM activity_intervals_active i
               JOIN activity_tasks t ON t.id = i.task_id
               LEFT JOIN activity_tasks parent ON parent.id = t.parent_task_id
               LEFT JOIN activity_interval_summaries s ON s.interval_id = i.id
               WHERE i.end_at > ?1
                 AND i.start_at < ?2
               ORDER BY i.start_at, i.end_at, i.id"#,
        )
        .bind(&start)
        .bind(&end)
        .fetch_all(&self.pool)
        .await?;
        let retention = self.retention_counts_for_range(&start, &end).await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let interval_retention = retention.get(&row.id).cloned().unwrap_or_default();
                row.into_record(interval_retention)
            })
            .collect())
    }

    /// On-demand raw evidence recall for one interval (the B03
    /// "summarize first, recall later" reader), newest-range order preserved.
    pub async fn activity_evidence_for_interval(
        &self,
        interval_id: i64,
        limit: i64,
    ) -> Result<Vec<ActivityEvidenceRecord>, SqlxError> {
        let rows = sqlx::query_as::<_, RawEvidence>(
            r#"SELECT e.interval_id, e.source_type, e.source_id, e.occurred_at,
                      CASE
                        WHEN e.source_type = 'frame' THEN source_frame.id
                        WHEN e.source_type = 'ui_event' THEN event_frame.id
                        ELSE NULL
                      END AS frame_id,
                      COALESCE(NULLIF(event.app_name, ''),
                               NULLIF(source_frame.app_name, ''),
                               NULLIF(event_frame.app_name, '')) AS app_name,
                      COALESCE(NULLIF(event.window_title, ''),
                               NULLIF(source_frame.window_name, ''),
                               NULLIF(event_frame.window_name, '')) AS window_title,
                      COALESCE(NULLIF(event.browser_url, ''),
                               NULLIF(source_frame.browser_url, ''),
                               NULLIF(event_frame.browser_url, '')) AS browser_url
               FROM activity_evidence e
               LEFT JOIN frames source_frame
                 ON e.source_type = 'frame' AND source_frame.id = e.source_id
               LEFT JOIN ui_events event
                 ON e.source_type = 'ui_event' AND event.id = e.source_id
               LEFT JOIN frames event_frame ON event_frame.id = event.frame_id
               WHERE e.interval_id = ?1
               ORDER BY e.occurred_at, e.id
               LIMIT ?2"#,
        )
        .bind(interval_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|item| ActivityEvidenceRecord {
                source_type: item.source_type,
                source_id: item.source_id,
                occurred_at: item.occurred_at,
                frame_id: item.frame_id,
                app_name: item.app_name,
                window_title: item.window_title,
                browser_url: item.browser_url,
            })
            .collect())
    }

    /// Intervals in range that have no summary yet, oldest first — the B02
    /// summarizer backlog. Summary/keyword fields are always `None` here.
    /// `settled_before` additionally requires `end_at <= settled_before` so
    /// the discovery only offers intervals that already ended (the B02a
    /// 5-minute grace window); `None` keeps the plain B01 behavior.
    pub async fn activity_intervals_missing_summary(
        &self,
        start_at: DateTime<Utc>,
        end_at: DateTime<Utc>,
        settled_before: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<ActivityIntervalRecord>, SqlxError> {
        self.activity_intervals_missing_summary_min_dwell(
            start_at,
            end_at,
            settled_before,
            limit,
            0,
        )
        .await
    }

    /// Same candidate scan, excluding intervals shorter than
    /// `min_dwell_seconds` — the ledger's short-segment floor (§4.4): absorbed
    /// shorts vanish at build time, and isolated shorts (no adjacent same-object
    /// main segment to absorb them) stay on the timeline but out of the summary
    /// candidate pool. Every pre-screen (§4.1.4: `final` state, settled grace,
    /// retained evidence, active version) is applied in SQL BEFORE the LIMIT,
    /// so stale candidates cannot permanently occupy the candidate page.
    pub async fn activity_intervals_missing_summary_min_dwell(
        &self,
        start_at: DateTime<Utc>,
        end_at: DateTime<Utc>,
        settled_before: Option<DateTime<Utc>>,
        limit: i64,
        min_dwell_seconds: i64,
    ) -> Result<Vec<ActivityIntervalRecord>, SqlxError> {
        let start = start_at.to_rfc3339();
        let end = end_at.to_rfc3339();
        let settled = settled_before.map(|t| t.to_rfc3339());
        let rows = sqlx::query_as::<_, RawIntervalDetail>(
            r#"SELECT i.id, t.id AS task_id, t.parent_task_id, t.kind, t.title,
                      parent.title AS parent_title, t.app_name,
                      i.start_at, i.end_at, i.state, i.confidence, i.producer,
                      (SELECT COUNT(*) FROM activity_evidence e
                        WHERE e.interval_id = i.id) AS evidence_count,
                      NULL AS summary, NULL AS keywords, NULL AS band
               FROM activity_intervals_active i
               JOIN activity_tasks t ON t.id = i.task_id
               LEFT JOIN activity_tasks parent ON parent.id = t.parent_task_id
               WHERE i.end_at > ?1
                 AND i.start_at < ?2
                 AND (?3 IS NULL OR i.end_at <= ?3)
                 AND (?4 <= 0 OR (julianday(i.end_at) - julianday(i.start_at)) * 86400.0 >= ?4)
                 AND i.state = 'final'
                 AND EXISTS (
                     SELECT 1 FROM activity_evidence ae WHERE ae.interval_id = i.id
                 )
                 AND NOT EXISTS (
                     SELECT 1 FROM activity_interval_summaries s
                     WHERE s.interval_id = i.id
                 )
               ORDER BY i.start_at, i.id
               LIMIT ?5"#,
        )
        .bind(&start)
        .bind(&end)
        .bind(settled)
        .bind(min_dwell_seconds)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| row.into_record(Vec::new()))
            .collect())
    }

    /// The stored summary for one interval when its `input_hash` still
    /// matches — the summarizer's idempotency read. `Some` means the exact
    /// same prompt/evidence/model inputs were already summarized and the
    /// model must not be called again; `None` (missing row or a different
    /// hash, e.g. after an evidence/prompt/model change) means the summary
    /// is still to be generated.
    pub async fn activity_summary_by_input_hash(
        &self,
        interval_id: i64,
        input_hash: &str,
    ) -> Result<Option<ActivitySummaryRow>, SqlxError> {
        let row: Option<(
            i64,
            String,
            String,
            String,
            i64,
            String,
            String,
            String,
            Option<String>,
            String,
        )> = sqlx::query_as(
            "SELECT interval_id, summary, keywords, band, summary_chars, evidence_refs, \
                        producer, prompt_version, model, input_hash \
                 FROM activity_interval_summaries \
                 WHERE interval_id = ?1 AND input_hash = ?2",
        )
        .bind(interval_id)
        .bind(input_hash)
        .fetch_optional(&self.pool)
        .await?;
        let Some((
            interval_id,
            summary,
            keywords,
            band,
            summary_chars,
            evidence_refs,
            producer,
            prompt_version,
            model,
            input_hash,
        )) = row
        else {
            return Ok(None);
        };
        Ok(Some(ActivitySummaryRow {
            interval_id,
            summary,
            keywords: serde_json::from_str(&keywords).unwrap_or_default(),
            band,
            summary_chars,
            evidence_refs: serde_json::from_str(&evidence_refs).unwrap_or_default(),
            producer,
            prompt_version,
            model,
            input_hash,
        }))
    }

    /// Insert or replace the summary of one interval. Citations must be 1–3
    /// rows that all exist in the interval's retained evidence — anything
    /// else is rejected before any write. An existing row with the same
    /// `input_hash` is returned untouched (idempotent re-run); a different
    /// hash replaces the row in place.
    pub async fn activity_summary_upsert(
        &self,
        interval_id: i64,
        summary: &str,
        keywords: &[String],
        band: &str,
        producer: &str,
        prompt_version: &str,
        model: Option<&str>,
        input_hash: &str,
        evidence_refs: &[ActivitySummaryEvidenceRef],
    ) -> Result<i64, SqlxError> {
        if summary.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "activity summary must not be empty".into(),
            ));
        }
        if !matches!(band, "short" | "medium" | "long") {
            return Err(SqlxError::Protocol(format!(
                "invalid activity summary band {band}"
            )));
        }
        if evidence_refs.is_empty() || evidence_refs.len() > 3 {
            return Err(SqlxError::Protocol(format!(
                "activity summary must cite 1–3 evidence rows, got {}",
                evidence_refs.len()
            )));
        }
        let mut tx = self.begin_immediate_with_retry().await?;
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT interval_id FROM activity_interval_summaries \
             WHERE interval_id = ?1 AND input_hash = ?2",
        )
        .bind(interval_id)
        .bind(input_hash)
        .fetch_optional(&mut **tx.conn())
        .await?;
        if let Some(id) = existing {
            tx.commit().await?;
            return Ok(id);
        }
        for reference in evidence_refs {
            let retained: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM activity_evidence \
                 WHERE interval_id = ?1 AND source_type = ?2 AND source_id = ?3",
            )
            .bind(interval_id)
            .bind(&reference.source_type)
            .bind(reference.source_id)
            .fetch_one(&mut **tx.conn())
            .await?;
            if retained == 0 {
                return Err(SqlxError::Protocol(format!(
                    "evidence ref {}/{} is not retained evidence of interval {}",
                    reference.source_type, reference.source_id, interval_id
                )));
            }
        }
        let summary_chars = summary.chars().filter(|c| !c.is_whitespace()).count() as i64;
        let keywords_json =
            serde_json::to_string(&keywords.to_vec()).unwrap_or_else(|_| "[]".into());
        let evidence_json =
            serde_json::to_string(&evidence_refs.to_vec()).unwrap_or_else(|_| "[]".into());
        let id: i64 = sqlx::query_scalar(
            r#"INSERT INTO activity_interval_summaries
               (interval_id, summary, keywords, band, summary_chars, evidence_refs,
                producer, prompt_version, model, input_hash)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
               ON CONFLICT(interval_id) DO UPDATE SET
                 summary = excluded.summary,
                 keywords = excluded.keywords,
                 band = excluded.band,
                 summary_chars = excluded.summary_chars,
                 evidence_refs = excluded.evidence_refs,
                 producer = excluded.producer,
                 prompt_version = excluded.prompt_version,
                 model = excluded.model,
                 input_hash = excluded.input_hash,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               RETURNING interval_id"#,
        )
        .bind(interval_id)
        .bind(summary)
        .bind(&keywords_json)
        .bind(band)
        .bind(summary_chars)
        .bind(&evidence_json)
        .bind(producer)
        .bind(prompt_version)
        .bind(model)
        .bind(input_hash)
        .fetch_one(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    async fn retention_counts_for_range(
        &self,
        start: &str,
        end: &str,
    ) -> Result<HashMap<i64, Vec<(String, i64, i64, Option<String>)>>, SqlxError> {
        let rows = sqlx::query_as::<_, (i64, String, i64, i64, Option<String>)>(
            r#"SELECT r.interval_id, r.source_type, r.kept, r.dropped, r.drop_reason
               FROM activity_interval_retention r
               JOIN activity_intervals_active i ON i.id = r.interval_id
               WHERE i.end_at > ?1
                 AND i.start_at < ?2
               ORDER BY r.interval_id, r.source_type"#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await?;
        let mut by_interval: HashMap<i64, Vec<(String, i64, i64, Option<String>)>> = HashMap::new();
        for (interval_id, source_type, kept, dropped, drop_reason) in rows {
            by_interval.entry(interval_id).or_default().push((
                source_type,
                kept,
                dropped,
                drop_reason,
            ));
        }
        Ok(by_interval)
    }
}

async fn upsert_task(
    tx: &mut ImmediateTx,
    producer: &str,
    task: &ActivityTaskDraft,
    parent_id: Option<i64>,
) -> Result<(), SqlxError> {
    sqlx::query(
        r#"INSERT INTO activity_tasks
           (task_key, parent_task_id, kind, title, app_name, confidence, producer)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
           ON CONFLICT(task_key) DO UPDATE SET
             parent_task_id = CASE WHEN activity_tasks.user_locked = 0
                                   THEN excluded.parent_task_id ELSE activity_tasks.parent_task_id END,
             kind = CASE WHEN activity_tasks.user_locked = 0
                         THEN excluded.kind ELSE activity_tasks.kind END,
             title = CASE WHEN activity_tasks.user_locked = 0
                          THEN excluded.title ELSE activity_tasks.title END,
             app_name = CASE WHEN activity_tasks.user_locked = 0
                             THEN excluded.app_name ELSE activity_tasks.app_name END,
             confidence = CASE WHEN activity_tasks.user_locked = 0
                               THEN excluded.confidence ELSE activity_tasks.confidence END,
             producer = excluded.producer,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
    )
    .bind(&task.task_key)
    .bind(parent_id)
    .bind(&task.kind)
    .bind(&task.title)
    .bind(&task.app_name)
    .bind(task.confidence.clamp(0.0, 1.0))
    .bind(producer)
    .execute(&mut **tx.conn())
    .await?;
    Ok(())
}

async fn task_id(tx: &mut ImmediateTx, task_key: &str) -> Result<i64, SqlxError> {
    sqlx::query_scalar("SELECT id FROM activity_tasks WHERE task_key = ?1")
        .bind(task_key)
        .fetch_one(&mut **tx.conn())
        .await
}

async fn cleanup_orphaned_tasks(tx: &mut ImmediateTx) -> Result<(), SqlxError> {
    sqlx::query(
        r#"DELETE FROM activity_tasks
           WHERE user_locked = 0
             AND NOT EXISTS (
                 SELECT 1 FROM activity_intervals i WHERE i.task_id = activity_tasks.id
             )
             AND NOT EXISTS (
                 SELECT 1 FROM activity_tasks child
                 WHERE child.parent_task_id = activity_tasks.id
             )"#,
    )
    .execute(&mut **tx.conn())
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenpipe_config::DbConfig;

    async fn test_db() -> (DatabaseManager, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = DatabaseManager::new(
            dir.path().join("db.sqlite").to_str().unwrap(),
            DbConfig::default(),
        )
        .await
        .unwrap();
        (db, dir)
    }

    fn at(value: &str) -> DateTime<Utc> {
        value.parse().unwrap()
    }

    async fn explain(db: &DatabaseManager, sql: &str) -> Vec<String> {
        let statement = format!("EXPLAIN QUERY PLAN {sql}");
        sqlx::query_as::<_, (i64, i64, i64, String)>(sqlx::AssertSqlSafe(statement))
            .fetch_all(&db.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.3)
            .collect()
    }

    fn drafts(source_id: i64) -> (Vec<ActivityTaskDraft>, Vec<ActivityIntervalDraft>) {
        let parent = ActivityTaskDraft {
            task_key: "parent".to_string(),
            parent_task_key: None,
            kind: "category".to_string(),
            title: "Browser".to_string(),
            app_name: Some("Browser".to_string()),
            confidence: 0.8,
        };
        let task = ActivityTaskDraft {
            task_key: "task".to_string(),
            parent_task_key: Some("parent".to_string()),
            kind: "task".to_string(),
            title: "Ticket 42".to_string(),
            app_name: Some("Browser".to_string()),
            confidence: 0.8,
        };
        let action = ActivityActionDraft {
            action_key: "action".to_string(),
            occurred_at: at("2026-08-17T09:01:00Z"),
            action_type: "click".to_string(),
            summary: "Clicked Reply".to_string(),
            app_name: Some("Browser".to_string()),
            confidence: 0.95,
            source_type: "ui_event".to_string(),
            source_id,
        };
        let interval = ActivityIntervalDraft {
            interval_key: "interval".to_string(),
            task_key: "task".to_string(),
            start_at: at("2026-08-17T09:00:00Z"),
            end_at: at("2026-08-17T09:05:00Z"),
            state: "final".to_string(),
            confidence: 0.8,
            actions: vec![action],
            evidence: vec![ActivityEvidenceDraft {
                source_type: "ui_event".to_string(),
                source_id,
                occurred_at: at("2026-08-17T09:01:00Z"),
                action_key: Some("action".to_string()),
            }],
            retention: HashMap::from([("ui_event".to_string(), (0, None))]),
        };
        (vec![parent, task], vec![interval])
    }

    #[tokio::test]
    async fn reconcile_round_trips_hierarchy_actions_and_evidence() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let frame_id: i64 = sqlx::query_scalar(
            "INSERT INTO frames \
             (timestamp, app_name, window_name, browser_url, focused) \
             VALUES ('2026-08-17T09:01:00Z', 'Arc', 'Pull request', \
                     'https://github.com/screenpipe/screenpipe/pull/42', 1) \
             RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events \
             (timestamp, relative_ms, event_type, frame_id, app_name, \
              window_title, browser_url) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', ?1, 'Arc', \
                     'Pull request', 'https://github.com/screenpipe/screenpipe/pull/42') \
             RETURNING id",
        )
        .bind(frame_id)
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, intervals) = drafts(source_id);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();

        let rows = db
            .list_activity_ledger(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T10:00:00Z"),
                true,
                true,
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Ticket 42");
        assert_eq!(rows[0].parent_title.as_deref(), Some("Browser"));
        assert_eq!(rows[0].actions.len(), 1);
        assert_eq!(rows[0].evidence.len(), 1);
        assert_eq!(rows[0].evidence[0].frame_id, Some(frame_id));
        assert_eq!(rows[0].evidence[0].app_name.as_deref(), Some("Arc"));
        assert_eq!(
            rows[0].evidence[0].browser_url.as_deref(),
            Some("https://github.com/screenpipe/screenpipe/pull/42")
        );
        assert_eq!(
            db.activity_ledger_last_reconciled_at("deterministic-v1")
                .await
                .unwrap(),
            Some(at("2026-08-17T09:10:00Z"))
        );
    }

    #[tokio::test]
    async fn reconcile_only_replaces_the_requested_trailing_range() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, mut intervals) = drafts(source_id);
        let mut later = intervals[0].clone();
        later.interval_key = "later-interval".to_string();
        later.start_at = at("2026-08-17T09:20:00Z");
        later.end_at = at("2026-08-17T09:25:00Z");
        later.actions.clear();
        intervals.push(later);

        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:30:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();

        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals[..1],
        )
        .await
        .unwrap();

        let rows = db
            .list_activity_ledger(
                at("2026-08-17T09:00:00Z"),
                at("2026-08-17T09:30:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].start_at, "2026-08-17T09:00:00+00:00");
        assert_eq!(rows[1].start_at, "2026-08-17T09:20:00+00:00");
    }

    #[tokio::test]
    async fn observation_query_combines_sampled_frames_ui_events_and_audio_metadata() {
        let (db, _dir) = test_db().await;
        db.execute_raw_sql_write(
            "INSERT INTO frames (timestamp, app_name, window_name, focused, document_path) \
             VALUES ('2026-08-17T09:00:00Z', 'Editor', 'main.rs', 1, '/tmp/main.rs')",
        )
        .await
        .unwrap();
        db.execute_raw_sql_write(
            "INSERT INTO ui_events \
             (timestamp, relative_ms, event_type, app_name, window_title, element_name) \
             VALUES ('2026-08-17T09:00:05Z', 0, 'click', 'Editor', 'main.rs', 'Run')",
        )
        .await
        .unwrap();
        db.execute_raw_sql_write("INSERT INTO audio_chunks (id, file_path) VALUES (1, 'test.wav')")
            .await
            .unwrap();
        db.execute_raw_sql_write(
            "INSERT INTO audio_transcriptions \
             (audio_chunk_id, offset_index, timestamp, transcription, device, is_input_device) \
             VALUES (1, 0, '2026-08-17T09:00:10Z', 'hello from the microphone', 'mic', 1)",
        )
        .await
        .unwrap();

        let rows = db
            .load_activity_ledger_observations(
                at("2026-08-17T09:00:00Z"),
                at("2026-08-17T09:01:00Z"),
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].source_type, "frame");
        assert_eq!(rows[0].document_path.as_deref(), Some("/tmp/main.rs"));
        assert_eq!(rows[1].event_type.as_deref(), Some("click"));
        assert_eq!(rows[2].source_type, "audio");
        assert_eq!(rows[2].is_input_device, Some(true));
    }

    #[tokio::test]
    async fn observation_times_keep_sub_millisecond_precision() {
        let (db, _dir) = test_db().await;
        // Two frames inside the same millisecond (100.2ms vs 100.8ms) with
        // different windows so the 10s sampler keeps both. Millisecond
        // ROUNDing used to collapse them onto one instant — and could carry
        // an event that happened just before an interval boundary onto the
        // boundary itself, breaking the [start_at, end_at) evidence contract.
        db.execute_raw_sql_write(
            "INSERT INTO frames (timestamp, app_name, window_name, focused) \
             VALUES ('2026-08-17T09:00:00.100200+00:00', 'Editor', 'main.rs', 1)",
        )
        .await
        .unwrap();
        db.execute_raw_sql_write(
            "INSERT INTO frames (timestamp, app_name, window_name, focused) \
             VALUES ('2026-08-17T09:00:00.100800+00:00', 'Editor', 'other.rs', 1)",
        )
        .await
        .unwrap();

        let rows = db
            .load_activity_ledger_observations(
                at("2026-08-17T09:00:00Z"),
                at("2026-08-17T09:01:00Z"),
            )
            .await
            .unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| (row.source_id, row.occurred_at))
                .collect::<Vec<_>>(),
            vec![
                (1, at("2026-08-17T09:00:00.100200Z")),
                (2, at("2026-08-17T09:00:00.100800Z")),
            ],
            "same-millisecond frames must stay distinct at microsecond precision"
        );
    }

    #[tokio::test]
    async fn activity_ledger_hot_ranges_seek_timestamp_indexes() {
        let (db, _dir) = test_db().await;
        let cases = [
            (
                "frames",
                "SELECT MIN(id) FROM frames \
                 WHERE timestamp >= '2026-08-17T09:00:00+00:00' \
                   AND timestamp < '2026-08-17T10:00:00+00:00' \
                 GROUP BY CAST(((julianday(timestamp) - 2440587.5) * 86400.0) / 10 AS INTEGER)",
            ),
            (
                "ui_events",
                "SELECT id FROM ui_events \
                 WHERE timestamp >= '2026-08-17T09:00:00+00:00' \
                   AND timestamp < '2026-08-17T10:00:00+00:00'",
            ),
            (
                "audio_transcriptions",
                "SELECT id FROM audio_transcriptions \
                 WHERE timestamp >= '2026-08-17T09:00:00+00:00' \
                   AND timestamp < '2026-08-17T10:00:00+00:00'",
            ),
            (
                "activity_intervals",
                "SELECT id FROM activity_intervals \
                 WHERE producer = 'deterministic-v1' \
                   AND start_at >= '2026-08-17T09:00:00+00:00'",
            ),
        ];

        for (table, sql) in cases {
            let plan = explain(&db, sql).await;
            assert!(
                plan.iter()
                    .any(|line| line.contains(&format!("SEARCH {table}"))),
                "{table} range did not use an index:\n{}",
                plan.join("\n")
            );
            assert!(
                !plan
                    .iter()
                    .any(|line| line.contains(&format!("SCAN {table}"))),
                "{table} range performed a full scan:\n{}",
                plan.join("\n")
            );
        }
    }

    #[tokio::test]
    async fn deleting_the_last_source_removes_unsupported_interval() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, intervals) = drafts(source_id);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();

        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        sqlx::query("DELETE FROM ui_events WHERE id = ?1")
            .bind(source_id)
            .execute(&mut **tx.conn())
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let rows = db
            .list_activity_ledger(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T10:00:00Z"),
                true,
                true,
            )
            .await
            .unwrap();
        assert!(rows.is_empty());
    }

    async fn insert_summary(db: &DatabaseManager, interval_id: i64, keywords: &str) {
        let sql = format!(
            "INSERT INTO activity_interval_summaries \
             (interval_id, summary, keywords, band, summary_chars, producer, \
              prompt_version, model, input_hash) \
             VALUES ({interval_id}, 'Worked on the ledger', '{keywords}', 'short', 20, \
                     'summarizer-v1', 'prompt-v1', 'test-model', 'hash-1')"
        );
        db.execute_raw_sql_write(&sql).await.unwrap();
    }

    #[tokio::test]
    async fn retention_bookkeeping_persisted_from_persisted_evidence() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, mut intervals) = drafts(source_id);
        // The draft claims two drops; `kept` must still come from the single
        // evidence row actually persisted for the interval.
        intervals[0].retention =
            HashMap::from([("ui_event".to_string(), (2, Some("unchanged".to_string())))]);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();

        let (kept, dropped, reason): (i64, i64, Option<String>) = sqlx::query_as(
            "SELECT kept, dropped, drop_reason FROM activity_interval_retention \
             WHERE interval_id = (SELECT id FROM activity_intervals \
                                   WHERE interval_key = 'interval') \
               AND source_type = 'ui_event'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(kept, 1);
        assert_eq!(dropped, 2);
        assert_eq!(reason.as_deref(), Some("unchanged"));
    }

    #[tokio::test]
    async fn retention_rows_and_summaries_cascade_with_interval() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, intervals) = drafts(source_id);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();
        let interval_id: i64 =
            sqlx::query_scalar("SELECT id FROM activity_intervals WHERE interval_key = 'interval'")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        insert_summary(&db, interval_id, "[]").await;

        // Deleting the interval must cascade both new tables away, exactly
        // like the evidence they describe.
        db.execute_raw_sql_write("DELETE FROM activity_intervals")
            .await
            .unwrap();
        let retention: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_interval_retention")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        let summaries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_interval_summaries")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(retention, 0);
        assert_eq!(summaries, 0);
    }

    #[tokio::test]
    async fn reconcile_rerun_keeps_retention_and_evidence_stable() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, intervals) = drafts(source_id);
        let range = (at("2026-08-17T09:00:00Z"), at("2026-08-17T09:10:00Z"));
        for _ in 0..2 {
            db.reconcile_activity_ledger("deterministic-v1", range.0, range.1, &tasks, &intervals)
                .await
                .unwrap();
        }

        let evidence: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity_evidence \
             WHERE interval_id = (SELECT id FROM activity_intervals \
                                   WHERE interval_key = 'interval')",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(evidence, 1, "kept evidence must survive a rerun");
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_interval_retention")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(rows, 1, "rerun must not duplicate retention rows");
    }

    #[tokio::test]
    async fn intervals_between_returns_summary_keywords_and_retention() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, intervals) = drafts(source_id);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();
        let interval_id: i64 =
            sqlx::query_scalar("SELECT id FROM activity_intervals WHERE interval_key = 'interval'")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        insert_summary(&db, interval_id, r#"["ledger","retention"]"#).await;

        let rows = db
            .activity_intervals_between(at("2026-08-17T08:00:00Z"), at("2026-08-17T10:00:00Z"))
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Ticket 42");
        assert_eq!(rows[0].summary.as_deref(), Some("Worked on the ledger"));
        assert_eq!(
            rows[0].keywords.as_deref(),
            Some(&["ledger".to_string(), "retention".to_string()][..])
        );
        assert_eq!(rows[0].summary_band.as_deref(), Some("short"));
        assert_eq!(
            rows[0].retention,
            vec![("ui_event".to_string(), 1, 0, None)]
        );
    }

    #[tokio::test]
    async fn evidence_for_interval_respects_limit_and_order() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, mut intervals) = drafts(source_id);
        for (index, minute) in [2, 3, 4].into_iter().enumerate() {
            intervals[0].evidence.push(ActivityEvidenceDraft {
                source_type: "ui_event".to_string(),
                source_id: source_id + index as i64 + 1,
                occurred_at: at(&format!("2026-08-17T09:{minute:02}:00Z")),
                action_key: None,
            });
        }
        // The extra evidence references source rows that do not exist; the
        // query joins enrich only, so insert matching ui_events first.
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        for index in 1..4 {
            sqlx::query(
                "INSERT INTO ui_events (id, timestamp, relative_ms, event_type, app_name) \
                 VALUES (?1, ?2, 0, 'text', 'Browser')",
            )
            .bind(source_id + index)
            .bind(&format!("2026-08-17T09:0{}:00Z", index + 1))
            .execute(&mut **tx.conn())
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();

        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();
        let interval_id: i64 =
            sqlx::query_scalar("SELECT id FROM activity_intervals WHERE interval_key = 'interval'")
                .fetch_one(&db.pool)
                .await
                .unwrap();

        let evidence = db
            .activity_evidence_for_interval(interval_id, 2)
            .await
            .unwrap();
        assert_eq!(evidence.len(), 2);
        assert_eq!(evidence[0].occurred_at, "2026-08-17T09:01:00+00:00");
        assert_eq!(evidence[1].occurred_at, "2026-08-17T09:02:00+00:00");
    }

    #[tokio::test]
    async fn missing_summary_excludes_summarized_intervals() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, mut intervals) = drafts(source_id);
        let mut later = intervals[0].clone();
        later.interval_key = "later-interval".to_string();
        later.start_at = at("2026-08-17T09:20:00Z");
        later.end_at = at("2026-08-17T09:25:00Z");
        intervals.push(later);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:30:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();
        let first_id: i64 =
            sqlx::query_scalar("SELECT id FROM activity_intervals WHERE interval_key = 'interval'")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        insert_summary(&db, first_id, "[]").await;

        let missing = db
            .activity_intervals_missing_summary(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T10:00:00Z"),
                None,
                10,
            )
            .await
            .unwrap();
        assert_eq!(missing.len(), 1);
        assert!(missing[0].summary.is_none());
        assert_eq!(missing[0].start_at, "2026-08-17T09:20:00+00:00");
    }

    #[tokio::test]
    async fn activity_missing_summary_settled_before_excludes_open_intervals() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, mut intervals) = drafts(source_id);
        // Ended well before the watermark: ready for summarization.
        intervals[0].start_at = at("2026-08-17T09:00:00Z");
        intervals[0].end_at = at("2026-08-17T09:05:00Z");
        // Still running: ends inside the grace window.
        let mut live = intervals[0].clone();
        live.interval_key = "live-interval".to_string();
        live.start_at = at("2026-08-17T09:20:00Z");
        live.end_at = at("2026-08-17T09:28:00Z");
        intervals.push(live);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:30:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();

        let ready = db
            .activity_intervals_missing_summary(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T09:25:00Z"),
                Some(at("2026-08-17T09:25:00Z")),
                10,
            )
            .await
            .unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].end_at, "2026-08-17T09:05:00+00:00");
    }

    #[tokio::test]
    async fn missing_summary_min_dwell_excludes_short_segments() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, mut intervals) = drafts(source_id);
        // A main segment (5 minutes) and an isolated short blip (10 seconds):
        // the dwell floor keeps only the main one as a summary candidate.
        intervals[0].start_at = at("2026-08-17T09:00:00Z");
        intervals[0].end_at = at("2026-08-17T09:05:00Z");
        let mut short = intervals[0].clone();
        short.interval_key = "short-blip".to_string();
        short.start_at = at("2026-08-17T09:20:00Z");
        short.end_at = at("2026-08-17T09:20:10Z");
        intervals.push(short);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:30:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();

        let ready = db
            .activity_intervals_missing_summary_min_dwell(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T10:00:00Z"),
                None,
                10,
                30,
            )
            .await
            .unwrap();
        assert_eq!(ready.len(), 1, "the 10s blip must stay out of candidates");
        assert_eq!(ready[0].start_at, "2026-08-17T09:00:00+00:00");

        // The legacy wrapper (floor 0) still sees both intervals.
        let all = db
            .activity_intervals_missing_summary(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T10:00:00Z"),
                None,
                10,
            )
            .await
            .unwrap();
        assert_eq!(all.len(), 2);
    }

    /// §4.4.4 attempt-3 counterexample (review JSON): rolling back HALF the
    /// window must switch coverage on boundary-complete bounds. Direct
    /// half-window activation previously left the preserved spanning v1
    /// segment and the surviving v2 tail BOTH visible (overlap), or a hole.
    #[tokio::test]
    async fn partial_rollback_switches_full_boundaries_without_overlap() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-09-12T00:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // v1: ONE spanning segment [00:00,00:10].
        let (v1_tasks, mut v1_intervals) = drafts(source_id);
        v1_intervals[0].interval_key = "v1-a".into();
        v1_intervals[0].start_at = at("2026-09-12T00:00:00Z");
        v1_intervals[0].end_at = at("2026-09-12T00:10:00Z");
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-09-12T00:00:00Z"),
            at("2026-09-12T00:10:00Z"),
            &v1_tasks,
            &v1_intervals,
        )
        .await
        .unwrap();

        // v2 rebuilds the same window as TWO segments.
        let (v2_tasks, v2_intervals) = two_segment_drafts(source_id, "v2");
        db.reconcile_activity_ledger(
            "deterministic-v2",
            at("2026-09-12T00:00:00Z"),
            at("2026-09-12T00:10:00Z"),
            &v2_tasks,
            &v2_intervals,
        )
        .await
        .unwrap();
        let before = db
            .list_activity_ledger(
                at("2026-09-11T23:00:00Z"),
                at("2026-09-12T01:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(before.len(), 2, "v2 halves visible, v1 hidden");

        // Roll back the FIRST half: must switch on the full [0,10] boundary.
        db.activity_ledger_activate_version(
            "deterministic-v1",
            at("2026-09-12T00:00:00Z"),
            at("2026-09-12T00:05:00Z"),
        )
        .await
        .unwrap();
        let list = db
            .list_activity_ledger(
                at("2026-09-11T23:00:00Z"),
                at("2026-09-12T01:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(list.len(), 1, "no overlap between versions: {list:?}");
        assert_eq!(list[0].producer, "deterministic-v1");
        assert_eq!(list[0].start_at, "2026-09-12T00:00:00+00:00");
        assert_eq!(list[0].end_at, "2026-09-12T00:10:00+00:00");

        // From the v2-active state, roll back the SECOND half: must widen to
        // the same full boundary instead of leaving a hole in [5,10).
        let (db2, _dir2) = test_db().await;
        let mut tx = db2.begin_immediate_with_retry().await.unwrap();
        let source_id2: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-09-12T00:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let (v1_tasks, mut v1_intervals) = drafts(source_id2);
        v1_intervals[0].interval_key = "v1-a".into();
        v1_intervals[0].start_at = at("2026-09-12T00:00:00Z");
        v1_intervals[0].end_at = at("2026-09-12T00:10:00Z");
        db2.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-09-12T00:00:00Z"),
            at("2026-09-12T00:10:00Z"),
            &v1_tasks,
            &v1_intervals,
        )
        .await
        .unwrap();
        let (v2_tasks, v2_intervals) = two_segment_drafts(source_id2, "v2");
        db2.reconcile_activity_ledger(
            "deterministic-v2",
            at("2026-09-12T00:00:00Z"),
            at("2026-09-12T00:10:00Z"),
            &v2_tasks,
            &v2_intervals,
        )
        .await
        .unwrap();
        db2.activity_ledger_activate_version(
            "deterministic-v1",
            at("2026-09-12T00:05:00Z"),
            at("2026-09-12T00:10:00Z"),
        )
        .await
        .unwrap();
        let list = db2
            .list_activity_ledger(
                at("2026-09-11T23:00:00Z"),
                at("2026-09-12T01:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(list.len(), 1, "no hole: full boundary switch: {list:?}");
        assert_eq!(list[0].producer, "deterministic-v1");
        assert_eq!(list[0].start_at, "2026-09-12T00:00:00+00:00");
        assert_eq!(list[0].end_at, "2026-09-12T00:10:00+00:00");
    }

    /// §4.4.4: activating a producer that does not actually cover the range
    /// would punch a hole — it must be rejected and leave coverage untouched.
    #[tokio::test]
    async fn activate_rejects_target_coverage_holes() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-09-12T00:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();
        // v1 only ever covered [00:00,00:05].
        let (v1_tasks, v1_intervals) = drafts(source_id);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-09-12T00:00:00Z"),
            at("2026-09-12T00:05:00Z"),
            &v1_tasks,
            &v1_intervals,
        )
        .await
        .unwrap();
        // v2 covers [00:00,00:10].
        let (v2_tasks, mut v2_intervals) = drafts(source_id);
        v2_intervals[0].interval_key = "v2-a".into();
        v2_intervals[0].end_at = at("2026-09-12T00:10:00Z");
        db.reconcile_activity_ledger(
            "deterministic-v2",
            at("2026-09-12T00:00:00Z"),
            at("2026-09-12T00:10:00Z"),
            &v2_tasks,
            &v2_intervals,
        )
        .await
        .unwrap();

        assert!(
            db.activity_ledger_activate_version(
                "deterministic-v1",
                at("2026-09-12T00:00:00Z"),
                at("2026-09-12T00:10:00Z"),
            )
            .await
            .is_err(),
            "hole in target coverage must be rejected"
        );
        let list = db
            .list_activity_ledger(
                at("2026-09-11T23:00:00Z"),
                at("2026-09-12T01:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].producer, "deterministic-v2", "coverage untouched");
    }

    /// §4.4.4 attempt-3: the DB reconcile entry itself guards boundaries — a
    /// rebuild range that bisects an existing segment is rejected instead of
    /// switching coverage over a partial span (engine pre-expansion alone is
    /// not trustable).
    #[tokio::test]
    async fn reconcile_rejects_straddling_rebuild_range() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-09-12T00:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let (v1_tasks, mut v1_intervals) = drafts(source_id);
        v1_intervals[0].interval_key = "v1-a".into();
        v1_intervals[0].start_at = at("2026-09-12T00:00:00Z");
        v1_intervals[0].end_at = at("2026-09-12T00:30:00Z");
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-09-12T00:00:00Z"),
            at("2026-09-12T00:30:00Z"),
            &v1_tasks,
            &v1_intervals,
        )
        .await
        .unwrap();

        // v2 rebuild over a range that bisects the v1 segment: rejected.
        let (v2_tasks, v2_intervals) = drafts(source_id);
        assert!(db
            .reconcile_activity_ledger(
                "deterministic-v2",
                at("2026-09-12T00:10:00Z"),
                at("2026-09-12T01:00:00Z"),
                &v2_tasks,
                &v2_intervals,
            )
            .await
            .is_err());
        let coverage: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity_ledger_coverage WHERE producer = 'deterministic-v2'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(coverage, 0, "no coverage switch on a straddling rebuild");
        let list = db
            .list_activity_ledger(
                at("2026-09-11T23:00:00Z"),
                at("2026-09-12T02:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].producer, "deterministic-v1");

        // Over the boundary-complete widened range the rebuild succeeds.
        let (mut v2_tasks, mut v2_intervals) = drafts(source_id);
        v2_intervals[0].interval_key = "v2-a".into();
        v2_intervals[0].start_at = at("2026-09-12T00:00:00Z");
        v2_intervals[0].end_at = at("2026-09-12T00:30:00Z");
        db.reconcile_activity_ledger(
            "deterministic-v2",
            at("2026-09-12T00:00:00Z"),
            at("2026-09-12T01:00:00Z"),
            &v2_tasks,
            &v2_intervals,
        )
        .await
        .unwrap();
        let list = db
            .list_activity_ledger(
                at("2026-09-11T23:00:00Z"),
                at("2026-09-12T02:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].producer, "deterministic-v2");
    }

    /// Rolling back the MIDDLE third across multi-segment versions: the
    /// switch must stay boundary-aligned (time-disjoint versions, no overlap,
    /// no hole) even when neither side straddles.
    #[tokio::test]
    async fn middle_third_rollback_stays_boundary_aligned() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-09-12T00:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (v1_tasks, v1_intervals) = three_segment_drafts(source_id, "v1");
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-09-12T00:00:00Z"),
            at("2026-09-12T00:09:00Z"),
            &v1_tasks,
            &v1_intervals,
        )
        .await
        .unwrap();
        let (v2_tasks, v2_intervals) = three_segment_drafts(source_id, "v2");
        db.reconcile_activity_ledger(
            "deterministic-v2",
            at("2026-09-12T00:00:00Z"),
            at("2026-09-12T00:09:00Z"),
            &v2_tasks,
            &v2_intervals,
        )
        .await
        .unwrap();

        db.activity_ledger_activate_version(
            "deterministic-v1",
            at("2026-09-12T00:03:00Z"),
            at("2026-09-12T00:06:00Z"),
        )
        .await
        .unwrap();
        let list = db
            .list_activity_ledger(
                at("2026-09-11T23:00:00Z"),
                at("2026-09-12T01:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(list.len(), 3, "three boundary-aligned segments: {list:?}");
        // Time-disjoint, no overlap, no hole.
        assert_eq!(list[0].start_at, "2026-09-12T00:00:00+00:00");
        assert_eq!(list[1].start_at, "2026-09-12T00:03:00+00:00");
        assert_eq!(list[2].start_at, "2026-09-12T00:06:00+00:00");
        assert_eq!(list[0].end_at, "2026-09-12T00:03:00+00:00");
        assert_eq!(list[1].end_at, "2026-09-12T00:06:00+00:00");
        assert_eq!(list[2].end_at, "2026-09-12T00:09:00+00:00");
        let producers: Vec<&str> = list.iter().map(|i| i.producer.as_str()).collect();
        assert_eq!(
            producers,
            ["deterministic-v2", "deterministic-v1", "deterministic-v2"]
        );
    }

    fn two_segment_drafts(
        source_id: i64,
        prefix: &str,
    ) -> (Vec<ActivityTaskDraft>, Vec<ActivityIntervalDraft>) {
        let (_, mut intervals) = drafts(source_id);
        let mut first = intervals[0].clone();
        let mut second = first.clone();
        first.interval_key = format!("{prefix}-a");
        first.start_at = at("2026-09-12T00:00:00Z");
        first.end_at = at("2026-09-12T00:05:00Z");
        second.interval_key = format!("{prefix}-b");
        second.start_at = at("2026-09-12T00:05:00Z");
        second.end_at = at("2026-09-12T00:10:00Z");
        second.actions = Vec::new();
        intervals = vec![first, second];
        let tasks = vec![ActivityTaskDraft {
            task_key: "task".to_string(),
            parent_task_key: Some("parent".to_string()),
            kind: "task".to_string(),
            title: "Ticket 42".to_string(),
            app_name: Some("Browser".to_string()),
            confidence: 0.8,
        }];
        (tasks, intervals)
    }

    fn three_segment_drafts(
        source_id: i64,
        prefix: &str,
    ) -> (Vec<ActivityTaskDraft>, Vec<ActivityIntervalDraft>) {
        let (_, intervals) = drafts(source_id);
        let mut segments = Vec::new();
        for (index, bounds) in [
            ("2026-09-12T00:00:00Z", "2026-09-12T00:03:00Z"),
            ("2026-09-12T00:03:00Z", "2026-09-12T00:06:00Z"),
            ("2026-09-12T00:06:00Z", "2026-09-12T00:09:00Z"),
        ]
        .into_iter()
        .enumerate()
        {
            let mut segment = intervals[0].clone();
            segment.interval_key = format!("{prefix}-{index}");
            segment.start_at = at(bounds.0);
            segment.end_at = at(bounds.1);
            segment.actions = Vec::new();
            segments.push(segment);
        }
        let tasks = vec![ActivityTaskDraft {
            task_key: "task".to_string(),
            parent_task_key: Some("parent".to_string()),
            kind: "task".to_string(),
            title: "Ticket 42".to_string(),
            app_name: Some("Browser".to_string()),
            confidence: 0.8,
        }];
        (tasks, segments)
    }

    /// §4.4.4: one active version per covered window — the default list and
    /// the summary discovery only see the newest producer, superseded rows
    /// stay queryable by id, and rollback is an explicit coverage switch.
    #[tokio::test]
    async fn active_version_switch_hides_superseded_producer_and_keeps_history() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // Both versions span the full window, so rollback coverage is real.
        let (v1_tasks, mut v1_intervals) = drafts(source_id);
        v1_intervals[0].start_at = at("2026-08-17T09:00:00Z");
        v1_intervals[0].end_at = at("2026-08-17T09:10:00Z");
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &v1_tasks,
            &v1_intervals,
        )
        .await
        .unwrap();
        // A v2 rebuild over the same window produces its own interval id.
        let (v2_tasks, mut v2_intervals) = drafts(source_id);
        v2_intervals[0].interval_key = "interval-v2".to_string();
        v2_intervals[0].start_at = at("2026-08-17T09:00:00Z");
        v2_intervals[0].end_at = at("2026-08-17T09:10:00Z");
        db.reconcile_activity_ledger(
            "deterministic-v2",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &v2_tasks,
            &v2_intervals,
        )
        .await
        .unwrap();

        let list = db
            .list_activity_ledger(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T10:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(list.len(), 1, "only the active version is listed");
        assert_eq!(list[0].producer, "deterministic-v2");
        let v2_key: String =
            sqlx::query_scalar("SELECT interval_key FROM activity_intervals WHERE id = ?1")
                .bind(list[0].id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(v2_key, "interval-v2");

        // Summary discovery is consistent with the default list.
        let missing = db
            .activity_intervals_missing_summary(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T10:00:00Z"),
                None,
                10,
            )
            .await
            .unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].producer, "deterministic-v2");

        // History stays queryable by id; the active recheck distinguishes.
        let v1_id: i64 =
            sqlx::query_scalar("SELECT id FROM activity_intervals WHERE interval_key = 'interval'")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        let v2_id: i64 = sqlx::query_scalar(
            "SELECT id FROM activity_intervals WHERE interval_key = 'interval-v2'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert!(!db.activity_interval_is_active(v1_id).await.unwrap());
        assert!(db.activity_interval_is_active(v2_id).await.unwrap());
        let history = db
            .activity_interval_record_by_id(v1_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(history.producer, "deterministic-v1");
        assert_eq!(
            db.activity_evidence_for_interval(v1_id, 10)
                .await
                .unwrap()
                .len(),
            1,
            "superseded evidence remains readable"
        );

        // Explicit rollback: the historical producer becomes active again by
        // appending a coverage row (recency = id, never version strings).
        db.activity_ledger_activate_version(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
        )
        .await
        .unwrap();
        let list = db
            .list_activity_ledger(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T10:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(
            list[0].producer, "deterministic-v1",
            "rollback switched back"
        );
    }

    /// §4.4.4 atomicity: a failed rebuild must leave the previous coverage —
    /// and therefore the previous active version — untouched.
    #[tokio::test]
    async fn failed_reconcile_leaves_previous_active_version_intact() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let (tasks, intervals) = drafts(source_id);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();

        // A v2 rebuild whose interval references an unknown task errors
        // mid-transaction.
        let bad = ActivityIntervalDraft {
            interval_key: "interval-bad".to_string(),
            task_key: "missing-task".to_string(),
            start_at: at("2026-08-17T09:00:00Z"),
            end_at: at("2026-08-17T09:05:00Z"),
            state: "final".to_string(),
            confidence: 0.8,
            actions: Vec::new(),
            evidence: Vec::new(),
            retention: HashMap::new(),
        };
        assert!(db
            .reconcile_activity_ledger(
                "deterministic-v2",
                at("2026-08-17T09:00:00Z"),
                at("2026-08-17T09:10:00Z"),
                &[],
                &[bad],
            )
            .await
            .is_err());

        let coverage: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity_ledger_coverage WHERE producer = 'deterministic-v2'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(coverage, 0, "no coverage switch on a failed rebuild");
        let list = db
            .list_activity_ledger(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T10:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].producer, "deterministic-v1", "old version intact");
    }

    /// §4.4.4: an empty rebuild still records coverage so the window cannot
    /// fall back to an older version and regenerate.
    #[tokio::test]
    async fn empty_window_rebuild_records_coverage() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let (tasks, intervals) = drafts(source_id);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();
        db.reconcile_activity_ledger(
            "deterministic-v2",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &[],
            &[],
        )
        .await
        .unwrap();

        let coverage: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity_ledger_coverage WHERE producer = 'deterministic-v2'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(coverage, 1, "the empty window is covered");
        let list = db
            .list_activity_ledger(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T10:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert!(list.is_empty(), "no fallback to the superseded version");
    }

    /// §4.4.4: repeated rebuilds of the same window converge — one visible
    /// copy and one superseding coverage row.
    #[tokio::test]
    async fn rebuild_is_idempotent_over_the_same_window() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();
        for _ in 0..2 {
            let (mut v2_tasks, mut v2_intervals) = drafts(source_id);
            v2_intervals[0].interval_key = "interval-v2".to_string();
            db.reconcile_activity_ledger(
                "deterministic-v2",
                at("2026-08-17T09:00:00Z"),
                at("2026-08-17T09:10:00Z"),
                &v2_tasks,
                &v2_intervals,
            )
            .await
            .unwrap();
        }
        let list = db
            .list_activity_ledger(
                at("2026-08-17T08:00:00Z"),
                at("2026-08-17T10:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(list.len(), 1, "no duplicate visible copies");
        let coverage: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity_ledger_coverage WHERE producer = 'deterministic-v2'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(coverage, 1, "the newer row supersedes the contained one");
    }

    /// §4.4.4 edge rule: a rebuild whose window bisects an existing segment
    /// is widened to that segment's full bounds, so the coverage switch never
    /// leaves a half-visible old segment nor loses the outside-window span.
    #[tokio::test]
    async fn expand_range_covers_straddling_interval() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T08:30:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let (tasks, mut intervals) = drafts(source_id);
        intervals[0].start_at = at("2026-08-17T08:00:00Z");
        intervals[0].end_at = at("2026-08-17T09:30:00Z");
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T08:00:00Z"),
            at("2026-08-17T09:30:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();

        let (start, end) = db
            .activity_ledger_expand_range_to_active_bounds(
                at("2026-08-17T08:30:00Z"),
                at("2026-08-17T10:30:00Z"),
                "deterministic-v1",
            )
            .await
            .unwrap();
        assert_eq!(start, at("2026-08-17T08:00:00Z"));
        assert_eq!(end, at("2026-08-17T10:30:00Z"));

        // Rebuilding v2 over the widened window replaces the whole straddler.
        let (mut v2_tasks, mut v2_intervals) = drafts(source_id);
        v2_intervals[0].start_at = at("2026-08-17T08:00:00Z");
        v2_intervals[0].end_at = at("2026-08-17T09:30:00Z");
        v2_intervals[0].interval_key = "interval-v2".to_string();
        db.reconcile_activity_ledger("deterministic-v2", start, end, &v2_tasks, &v2_intervals)
            .await
            .unwrap();
        let list = db
            .list_activity_ledger(
                at("2026-08-17T07:00:00Z"),
                at("2026-08-17T11:00:00Z"),
                false,
                false,
            )
            .await
            .unwrap();
        assert_eq!(
            list.len(),
            1,
            "the straddler is fully replaced, not half-hidden"
        );
        assert_eq!(list[0].producer, "deterministic-v2");
        assert_eq!(list[0].start_at, "2026-08-17T08:00:00+00:00");
    }

    #[tokio::test]
    async fn activity_summary_upsert_rejects_out_of_range_evidence_refs() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, intervals) = drafts(source_id);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();
        let interval_id: i64 =
            sqlx::query_scalar("SELECT id FROM activity_intervals WHERE interval_key = 'interval'")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        let reference = |source_type: &str, source_id: i64| ActivitySummaryEvidenceRef {
            source_type: source_type.to_string(),
            source_id,
        };
        let valid = vec![reference("ui_event", source_id)];

        // No citation at all.
        let error = db
            .activity_summary_upsert(
                interval_id,
                "摘要正文",
                &["关键字".to_string()],
                "short",
                "summarizer-v1",
                "prompt-v1",
                Some("test-model"),
                "hash-a",
                &[],
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("1–3"));

        // More than three citations.
        let four = vec![
            reference("ui_event", source_id),
            reference("ui_event", source_id),
            reference("ui_event", source_id),
            reference("ui_event", source_id),
        ];
        let error = db
            .activity_summary_upsert(
                interval_id,
                "摘要正文",
                &["关键字".to_string()],
                "short",
                "summarizer-v1",
                "prompt-v1",
                Some("test-model"),
                "hash-a",
                &four,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("1–3"));

        // Citation pointing at evidence that was never retained for the
        // interval (wrong source id and wrong source type).
        for forged in [
            vec![reference("ui_event", source_id + 999)],
            vec![reference("frame", source_id)],
        ] {
            let error = db
                .activity_summary_upsert(
                    interval_id,
                    "摘要正文",
                    &["关键字".to_string()],
                    "short",
                    "summarizer-v1",
                    "prompt-v1",
                    Some("test-model"),
                    "hash-a",
                    &forged,
                )
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("not retained evidence"),
                "forged citation must be rejected: {error}"
            );
        }

        // Invalid band is rejected as well.
        let error = db
            .activity_summary_upsert(
                interval_id,
                "摘要正文",
                &["关键字".to_string()],
                "epic",
                "summarizer-v1",
                "prompt-v1",
                Some("test-model"),
                "hash-a",
                &valid,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("band"));

        // Nothing was written by any rejected call.
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_interval_summaries")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(rows, 0);

        // One retained citation passes.
        db.activity_summary_upsert(
            interval_id,
            "在 Browser 里修复 Ticket 42 的回复流程",
            &["Ticket 42".to_string()],
            "short",
            "summarizer-v1",
            "prompt-v1",
            Some("test-model"),
            "hash-a",
            &valid,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn activity_summary_upsert_is_idempotent_per_input_hash() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, intervals) = drafts(source_id);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();
        let interval_id: i64 =
            sqlx::query_scalar("SELECT id FROM activity_intervals WHERE interval_key = 'interval'")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        let refs = vec![ActivitySummaryEvidenceRef {
            source_type: "ui_event".to_string(),
            source_id,
        }];
        let keywords = vec!["Ticket 42".to_string(), "Browser".to_string()];

        let first_id = db
            .activity_summary_upsert(
                interval_id,
                "在 Browser 中回复 Ticket 42 并检查附件",
                &keywords,
                "short",
                "summarizer-v1",
                "prompt-v1",
                Some("test-model"),
                "hash-1",
                &refs,
            )
            .await
            .unwrap();
        let first_row = db
            .activity_summary_by_input_hash(interval_id, "hash-1")
            .await
            .unwrap()
            .expect("row stored");
        assert_eq!(
            first_row.summary_chars as usize,
            "在 Browser 中回复 Ticket 42 并检查附件"
                .chars()
                .filter(|c| !c.is_whitespace())
                .count()
        );

        // Same input hash: nothing is rewritten.
        let second_id = db
            .activity_summary_upsert(
                interval_id,
                "完全不同的另一段摘要内容也无所谓",
                &keywords,
                "short",
                "summarizer-v1",
                "prompt-v1",
                Some("test-model"),
                "hash-1",
                &refs,
            )
            .await
            .unwrap();
        assert_eq!(first_id, second_id);
        let unchanged = db
            .activity_summary_by_input_hash(interval_id, "hash-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.summary, first_row.summary);
        assert_eq!(unchanged.summary_chars, first_row.summary_chars);

        // A different hash replaces the row in place — still one row.
        let replaced_id = db
            .activity_summary_upsert(
                interval_id,
                "证据变化后重新生成的摘要正文内容",
                &keywords,
                "short",
                "summarizer-v1",
                "prompt-v2",
                Some("test-model-2"),
                "hash-2",
                &refs,
            )
            .await
            .unwrap();
        assert_eq!(replaced_id, first_id);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_interval_summaries")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        let old_hash = db
            .activity_summary_by_input_hash(interval_id, "hash-1")
            .await
            .unwrap();
        assert!(old_hash.is_none(), "stale input hash must not match");
    }

    #[tokio::test]
    async fn activity_summary_by_input_hash_reports_pending_generation() {
        let (db, _dir) = test_db().await;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO ui_events (timestamp, relative_ms, event_type, app_name) \
             VALUES ('2026-08-17T09:01:00Z', 0, 'click', 'Browser') RETURNING id",
        )
        .fetch_one(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let (tasks, intervals) = drafts(source_id);
        db.reconcile_activity_ledger(
            "deterministic-v1",
            at("2026-08-17T09:00:00Z"),
            at("2026-08-17T09:10:00Z"),
            &tasks,
            &intervals,
        )
        .await
        .unwrap();
        let interval_id: i64 =
            sqlx::query_scalar("SELECT id FROM activity_intervals WHERE interval_key = 'interval'")
                .fetch_one(&db.pool)
                .await
                .unwrap();

        // No row at all: to be generated.
        assert!(db
            .activity_summary_by_input_hash(interval_id, "hash-1")
            .await
            .unwrap()
            .is_none());

        db.activity_summary_upsert(
            interval_id,
            "在 Browser 中回复 Ticket 42 并检查附件",
            &["Ticket 42".to_string()],
            "short",
            "summarizer-v1",
            "prompt-v1",
            Some("test-model"),
            "hash-1",
            &[ActivitySummaryEvidenceRef {
                source_type: "ui_event".to_string(),
                source_id,
            }],
        )
        .await
        .unwrap();

        // Same hash: already generated; different hash (e.g. evidence or
        // model changed): to be generated again.
        assert!(db
            .activity_summary_by_input_hash(interval_id, "hash-1")
            .await
            .unwrap()
            .is_some());
        assert!(db
            .activity_summary_by_input_hash(interval_id, "hash-2")
            .await
            .unwrap()
            .is_none());

        let row = db
            .activity_summary_by_input_hash(interval_id, "hash-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.evidence_refs.len(), 1);
        assert_eq!(row.evidence_refs[0].source_type, "ui_event");
        assert_eq!(row.evidence_refs[0].source_id, source_id);
        assert_eq!(row.band, "short");
        assert_eq!(row.model.as_deref(), Some("test-model"));
    }

    #[tokio::test]
    async fn frame_fingerprint_prefers_content_hash_and_falls_back_to_text() {
        let (db, _dir) = test_db().await;
        db.execute_raw_sql_write(
            "INSERT INTO frames \
             (timestamp, app_name, window_name, focused, content_hash) \
             VALUES ('2026-08-17T09:00:00Z', 'Editor', 'a', 1, 123456789)",
        )
        .await
        .unwrap();
        db.execute_raw_sql_write(
            "INSERT INTO frames \
             (timestamp, app_name, window_name, focused, full_text) \
             VALUES ('2026-08-17T09:00:20Z', 'Editor', 'b', 1, 'hello world text')",
        )
        .await
        .unwrap();
        db.execute_raw_sql_write(
            "INSERT INTO frames \
             (timestamp, app_name, window_name, focused, accessibility_text) \
             VALUES ('2026-08-17T09:00:40Z', 'Editor', 'c', 1, 'ax body')",
        )
        .await
        .unwrap();
        db.execute_raw_sql_write(
            "INSERT INTO frames \
             (timestamp, app_name, window_name, focused) \
             VALUES ('2026-08-17T09:01:00Z', 'Editor', 'd', 1)",
        )
        .await
        .unwrap();
        db.execute_raw_sql_write("INSERT INTO audio_chunks (id, file_path) VALUES (1, 'test.wav')")
            .await
            .unwrap();
        db.execute_raw_sql_write(
            "INSERT INTO audio_transcriptions \
             (audio_chunk_id, offset_index, timestamp, transcription, device, is_input_device) \
             VALUES (1, 0, '2026-08-17T09:00:10Z', '  ', 'mic', 1)",
        )
        .await
        .unwrap();

        let rows = db
            .load_activity_ledger_observations(
                at("2026-08-17T09:00:00Z"),
                at("2026-08-17T09:02:00Z"),
            )
            .await
            .unwrap();

        let by_key: HashMap<(String, i64), &ActivityLedgerObservation> = rows
            .iter()
            .map(|row| ((row.source_type.clone(), row.source_id), row))
            .collect();
        let hashed = by_key.get(&("frame".to_string(), 1)).unwrap();
        assert_eq!(hashed.content_fingerprint.as_deref(), Some("123456789"));
        assert!(!hashed.content_empty);

        let from_full_text = by_key.get(&("frame".to_string(), 2)).unwrap();
        let fingerprint = from_full_text.content_fingerprint.as_deref().unwrap();
        assert!(fingerprint.starts_with("t16:"), "{fingerprint}");
        assert!(!from_full_text.content_empty);

        let from_accessibility = by_key.get(&("frame".to_string(), 3)).unwrap();
        assert!(from_accessibility.content_fingerprint.is_some());
        assert!(!from_accessibility.content_empty);

        let blank = by_key.get(&("frame".to_string(), 4)).unwrap();
        assert_eq!(blank.content_fingerprint, None);
        assert!(blank.content_empty);

        // The whitespace-only transcript now flows through, truthfully marked
        // empty, instead of being pre-filtered by the loader.
        let empty_audio = rows
            .iter()
            .find(|row| row.source_type == "audio")
            .expect("empty transcript must reach the policy");
        assert!(empty_audio.content_empty);
        assert_eq!(empty_audio.content_fingerprint, None);
    }

    #[tokio::test]
    async fn meeting_spans_clamp_to_range_and_include_open_meetings() {
        let (db, _dir) = test_db().await;
        db.execute_raw_sql_write(
            "INSERT INTO meetings (meeting_start, meeting_end, meeting_app, title) \
             VALUES ('2026-08-17T08:50:00Z', '2026-08-17T09:05:00Z', 'Meet', 'ended')",
        )
        .await
        .unwrap();
        db.execute_raw_sql_write(
            "INSERT INTO meetings (meeting_start, meeting_end, meeting_app, title) \
             VALUES ('2026-08-17T09:20:00Z', NULL, 'Zoom', 'open')",
        )
        .await
        .unwrap();
        db.execute_raw_sql_write(
            "INSERT INTO meetings (meeting_start, meeting_end, meeting_app, title) \
             VALUES ('2026-08-17T10:00:00Z', '2026-08-17T11:00:00Z', 'Meet', 'later')",
        )
        .await
        .unwrap();

        let spans = db
            .activity_meeting_spans(at("2026-08-17T09:00:00Z"), at("2026-08-17T09:30:00Z"))
            .await
            .unwrap();
        assert_eq!(
            spans,
            vec![
                (at("2026-08-17T09:00:00Z"), at("2026-08-17T09:05:00Z")),
                (at("2026-08-17T09:20:00Z"), at("2026-08-17T09:30:00Z")),
            ]
        );
    }
}
