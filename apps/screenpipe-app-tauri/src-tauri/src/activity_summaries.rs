// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Read face for the activity timeline's database summaries (B02b-1 读源收敛).
//!
//! `get_activity_interval_summaries` serves the interval summaries produced by
//! B01/B02a straight from the shared activity database, shaped to the
//! frontend's existing `ActivityHistoryEntry` contract so the timeline can
//! prefer database rows and only fall back to the legacy KV narrative when
//! this read is empty or fails. The legacy generator in `activity_history.rs`
//! keeps its behavior (its retirement is B02b-2).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use screenpipe_db::{ActivityIntervalRecord, DatabaseManager};
use serde::Serialize;
use specta::Type;

/// Page cap for one read; `coverage.truncated` tells the reader when a range
/// was cut short.
const DEFAULT_SUMMARY_LIMIT: u32 = 500;
/// Evidence cited per entry mirrors the legacy generator's 1–3 budget.
const MAX_SUMMARY_EVIDENCE: usize = 3;
/// An interval counts as a meeting when at least half of it is covered by a
/// recorded meeting span (the B01 `activity_meeting_spans` criterion).
const MEETING_OVERLAP_DENOMINATOR: i64 = 2;

#[derive(Debug, Clone, Serialize, Type)]
pub struct ActivityIntervalSummaryEvidence {
    pub source_type: String,
    pub source_id: i64,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ActivityIntervalSummaryEntry {
    pub id: String,
    pub kind: String,
    pub start_at: String,
    pub end_at: String,
    pub title: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub evidence: Vec<ActivityIntervalSummaryEvidence>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ActivityIntervalSummariesCoverage {
    pub start: String,
    pub end: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ActivityIntervalSummariesResponse {
    pub entries: Vec<ActivityIntervalSummaryEntry>,
    pub coverage: ActivityIntervalSummariesCoverage,
}

fn shared_activity_db() -> Result<Arc<DatabaseManager>, String> {
    // The engine publishes its shared database shortly after native startup;
    // until then the timeline falls back to the legacy KV narrative.
    screenpipe_engine::knowledge::shared()
        .map(|shared| Arc::clone(&shared.db))
        .ok_or_else(|| "activity database is not available yet".to_string())
}

fn parse_summary_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn parse_summary_range(start: &str, end: &str) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
    let start =
        parse_summary_time(start).ok_or_else(|| "Invalid activity start time".to_string())?;
    let end = parse_summary_time(end).ok_or_else(|| "Invalid activity end time".to_string())?;
    if start >= end {
        return Err("Start time must be before end time".to_string());
    }
    Ok((start, end))
}

/// Range criterion for a page: an interval belongs to it when it starts inside
/// `[start, end)`. The ledger query itself is overlap-based, so this trims
/// intervals that merely reach into the range from earlier pages.
fn interval_starts_in_range(
    record: &ActivityIntervalRecord,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> bool {
    parse_summary_time(&record.start_at).is_some_and(|interval_start| {
        interval_start >= start && interval_start < end
    })
}

/// Meeting criterion fixed on the B01 spans read: the recorded meeting
/// coverage of an interval decides `meeting` only at over-half overlap.
fn interval_kind(
    record: &ActivityIntervalRecord,
    meeting_spans: &[(DateTime<Utc>, DateTime<Utc>)],
) -> &'static str {
    let (Some(interval_start), Some(interval_end)) = (
        parse_summary_time(&record.start_at),
        parse_summary_time(&record.end_at),
    ) else {
        return "work";
    };
    let duration_ms = (interval_end - interval_start).num_milliseconds();
    if duration_ms <= 0 {
        return "work";
    }
    let overlap_ms = meeting_spans
        .iter()
        .map(|(span_start, span_end)| {
            (interval_end.min(*span_end) - interval_start.max(*span_start)).num_milliseconds()
                .max(0)
        })
        .sum::<i64>();
    if overlap_ms * MEETING_OVERLAP_DENOMINATOR >= duration_ms {
        "meeting"
    } else {
        "work"
    }
}

async fn interval_summary_evidence(
    db: &DatabaseManager,
    record: &ActivityIntervalRecord,
) -> Result<Vec<ActivityIntervalSummaryEvidence>, String> {
    let rows = db
        .activity_evidence_for_interval(record.id, MAX_SUMMARY_EVIDENCE as i64)
        .await
        .map_err(|error| format!("activity evidence read failed: {error}"))?;
    Ok(rows
        .into_iter()
        .take(MAX_SUMMARY_EVIDENCE)
        .map(|row| ActivityIntervalSummaryEvidence {
            source_type: row.source_type,
            source_id: row.source_id,
            occurred_at: row.occurred_at,
        })
        .collect())
}

async fn load_interval_summaries(
    db: &DatabaseManager,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    limit: u32,
) -> Result<ActivityIntervalSummariesResponse, String> {
    let meeting_spans = db
        .activity_meeting_spans(start, end)
        .await
        .map_err(|error| format!("activity meeting spans failed: {error}"))?;
    let records = db
        .activity_intervals_between(start, end)
        .await
        .map_err(|error| format!("activity intervals read failed: {error}"))?;

    let mut entries = Vec::new();
    for record in &records {
        // No persisted summary yet → the interval is not served; the timeline
        // falls back to the KV narrative as a whole when nothing is ready.
        let Some(summary) = record.summary.as_deref().map(str::trim).filter(|summary| {
            !summary.is_empty()
        }) else {
            continue;
        };
        if !interval_starts_in_range(record, start, end) {
            continue;
        }
        let kind = interval_kind(record, &meeting_spans);
        entries.push(ActivityIntervalSummaryEntry {
            id: format!("{kind}:{}", record.id),
            kind: kind.to_string(),
            start_at: record.start_at.clone(),
            end_at: record.end_at.clone(),
            title: record.title.clone(),
            summary: summary.to_string(),
            keywords: record.keywords.clone().unwrap_or_default(),
            evidence: interval_summary_evidence(db, record).await?,
        });
    }

    let truncated = entries.len() > limit as usize;
    entries.truncate(limit as usize);
    Ok(ActivityIntervalSummariesResponse {
        entries,
        coverage: ActivityIntervalSummariesCoverage {
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
            truncated,
        },
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_activity_interval_summaries(
    start: String,
    end: String,
    limit: Option<u32>,
) -> Result<ActivityIntervalSummariesResponse, String> {
    let (start, end) = parse_summary_range(&start, &end)?;
    let limit = limit.unwrap_or(DEFAULT_SUMMARY_LIMIT).max(1);
    let db = shared_activity_db()?;
    load_interval_summaries(&db, start, end, limit).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenpipe_config::DbConfig;
    use screenpipe_db::{
        ActivityEvidenceDraft, ActivityIntervalDraft, ActivitySummaryEvidenceRef, ActivityTaskDraft,
    };
    use std::collections::HashMap;

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

    fn rfc3339(value: DateTime<Utc>) -> String {
        value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    /// Persist one interval with retained frame evidence and an optional
    /// summary, mirroring the B01/B02a producer writes.
    async fn seed_interval(
        db: &DatabaseManager,
        producer: &str,
        task_key: &str,
        task_title: &str,
        interval_key: &str,
        start_at: &str,
        end_at: &str,
        evidence_times: &[&str],
        summary: Option<(&str, &str, &str)>,
    ) -> i64 {
        db.reconcile_activity_ledger(
            producer,
            at(start_at),
            at(end_at),
            &[ActivityTaskDraft {
                task_key: task_key.to_string(),
                parent_task_key: None,
                kind: "task".to_string(),
                title: task_title.to_string(),
                app_name: Some("Cursor".to_string()),
                confidence: 0.9,
            }],
            &[ActivityIntervalDraft {
                interval_key: interval_key.to_string(),
                task_key: task_key.to_string(),
                start_at: at(start_at),
                end_at: at(end_at),
                state: "final".to_string(),
                confidence: 0.9,
                actions: Vec::new(),
                evidence: evidence_times
                    .iter()
                    .enumerate()
                    .map(|(index, occurred_at)| ActivityEvidenceDraft {
                        source_type: "frame".to_string(),
                        source_id: 1000 + index as i64,
                        occurred_at: at(occurred_at),
                        action_key: None,
                    })
                    .collect(),
                retention: HashMap::new(),
            }],
        )
        .await
        .unwrap();
        // The write face does not return ids; read them back through the B01
        // read face instead of reaching for raw SQL (no sqlx in this crate).
        let interval_id = db
            .activity_intervals_between(at(start_at), at(end_at))
            .await
            .unwrap()
            .into_iter()
            .find(|record| record.start_at == at(start_at).to_rfc3339())
            .unwrap_or_else(|| panic!("seeded interval {interval_key} not found"))
            .id;
        if let Some((summary, keywords, band)) = summary {
            let keywords: Vec<String> =
                serde_json::from_str(keywords).expect("fixture keywords json");
            let refs: Vec<ActivitySummaryEvidenceRef> = (0..evidence_times.len())
                .map(|index| ActivitySummaryEvidenceRef {
                    source_type: "frame".to_string(),
                    source_id: 1000 + index as i64,
                })
                .collect();
            db.activity_summary_upsert(
                interval_id,
                summary,
                &keywords,
                band,
                "fixture-producer",
                "fixture-prompt-v1",
                Some("fixture-model"),
                "fixture-input-hash",
                &refs,
            )
            .await
            .unwrap();
        }
        interval_id
    }

    async fn seed_meeting(db: &DatabaseManager, start_at: &str, end_at: &str) -> i64 {
        let meeting_id = db
            .insert_meeting_with_calendar_at(
                "Zoom",
                "fixture",
                Some("Planning"),
                None,
                None,
                at(start_at),
            )
            .await
            .unwrap();
        db.end_meeting(meeting_id, &rfc3339(at(end_at)), None)
            .await
            .unwrap();
        meeting_id
    }

    fn entry_ids(response: &ActivityIntervalSummariesResponse) -> Vec<&str> {
        response
            .entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect()
    }

    #[test]
    fn summary_range_rejects_invalid_and_reversed_bounds() {
        assert_eq!(
            parse_summary_range("not-a-time", "2026-09-11T10:00:00Z").unwrap_err(),
            "Invalid activity start time"
        );
        assert_eq!(
            parse_summary_range("2026-09-11T10:00:00Z", "also-invalid").unwrap_err(),
            "Invalid activity end time"
        );
        assert_eq!(
            parse_summary_range("2026-09-11T10:00:00Z", "2026-09-11T10:00:00Z").unwrap_err(),
            "Start time must be before end time"
        );
    }

    #[test]
    fn range_criterion_keeps_only_intervals_starting_inside_the_page() {
        let record = |start_at: &str| ActivityIntervalRecord {
            id: 1,
            task_id: 1,
            parent_task_id: None,
            kind: "task".to_string(),
            title: "t".to_string(),
            parent_title: None,
            app_name: None,
            start_at: start_at.to_string(),
            end_at: "2026-09-11T12:00:00Z".to_string(),
            state: "final".to_string(),
            confidence: 1.0,
            producer: "p".to_string(),
            evidence_count: 0,
            actions: Vec::new(),
            evidence: Vec::new(),
            summary: None,
            keywords: None,
            summary_band: None,
            retention: Vec::new(),
        };
        let start = at("2026-09-11T10:00:00Z");
        let end = at("2026-09-11T11:00:00Z");

        // Inclusive start boundary, exclusive end boundary.
        assert!(interval_starts_in_range(&record("2026-09-11T10:00:00Z"), start, end));
        assert!(interval_starts_in_range(&record("2026-09-11T10:59:59Z"), start, end));
        // Overlapping from an earlier page or starting at/after the end.
        assert!(!interval_starts_in_range(&record("2026-09-11T09:59:59Z"), start, end));
        assert!(!interval_starts_in_range(&record("2026-09-11T11:00:00Z"), start, end));
        // Unparseable timestamps never count as in range.
        assert!(!interval_starts_in_range(&record("garbage"), start, end));
    }

    #[test]
    fn meeting_kind_requires_more_than_half_span_coverage() {
        let record = ActivityIntervalRecord {
            id: 1,
            task_id: 1,
            parent_task_id: None,
            kind: "task".to_string(),
            title: "t".to_string(),
            parent_title: None,
            app_name: None,
            start_at: "2026-09-11T10:00:00Z".to_string(),
            end_at: "2026-09-11T10:40:00Z".to_string(),
            state: "final".to_string(),
            confidence: 1.0,
            producer: "p".to_string(),
            evidence_count: 0,
            actions: Vec::new(),
            evidence: Vec::new(),
            summary: None,
            keywords: None,
            summary_band: None,
            retention: Vec::new(),
        };
        let span = |start_at: &str, end_at: &str| (at(start_at), at(end_at));

        // 30 of 40 minutes covered: over half → meeting.
        assert_eq!(
            interval_kind(&record, &[span("2026-09-11T10:10:00Z", "2026-09-11T10:40:00Z")]),
            "meeting"
        );
        // Exactly half counts as meeting ("重叠过半" at the >= boundary).
        assert_eq!(
            interval_kind(&record, &[span("2026-09-11T10:20:00Z", "2026-09-11T10:40:00Z")]),
            "meeting"
        );
        // 19 of 40 minutes: below half → work. Multiple spans accumulate.
        assert_eq!(
            interval_kind(&record, &[span("2026-09-11T10:21:00Z", "2026-09-11T10:40:00Z")]),
            "work"
        );
        assert_eq!(
            interval_kind(
                &record,
                &[
                    span("2026-09-11T10:00:00Z", "2026-09-11T10:10:00Z"),
                    span("2026-09-11T10:30:00Z", "2026-09-11T10:40:00Z")
                ]
            ),
            "meeting"
        );
        assert_eq!(interval_kind(&record, &[]), "work");
    }

    #[tokio::test]
    async fn summaries_map_interval_fields_keywords_and_evidence_times() {
        let (db, _dir) = test_db().await;
        seed_interval(
            &db,
            "fixture",
            "task-1",
            "修复了时间线读取",
            "interval-1",
            "2026-09-11T10:00:00Z",
            "2026-09-11T10:30:00Z",
            &["2026-09-11T10:05:00Z", "2026-09-11T10:20:00Z"],
            Some((
                "把时间线改成优先读数据库摘要。",
                r#"["读源收敛","数据库摘要"]"#,
                "medium",
            )),
        )
        .await;

        let response = load_interval_summaries(
            &db,
            at("2026-09-11T10:00:00Z"),
            at("2026-09-11T11:00:00Z"),
            DEFAULT_SUMMARY_LIMIT,
        )
        .await
        .unwrap();

        assert_eq!(entry_ids(&response), ["work:1"]);
        let entry = &response.entries[0];
        assert_eq!(entry.kind, "work");
        // Timestamps pass through verbatim from the database rows.
        assert_eq!(entry.start_at, "2026-09-11T10:00:00+00:00");
        assert_eq!(entry.end_at, "2026-09-11T10:30:00+00:00");
        assert_eq!(entry.title, "修复了时间线读取");
        assert_eq!(entry.summary, "把时间线改成优先读数据库摘要。");
        assert_eq!(entry.keywords, vec!["读源收敛", "数据库摘要"]);
        assert_eq!(entry.evidence.len(), 2);
        assert_eq!(entry.evidence[0].source_type, "frame");
        assert_eq!(entry.evidence[0].source_id, 1000);
        assert_eq!(entry.evidence[0].occurred_at, "2026-09-11T10:05:00+00:00");
        assert!(!response.coverage.truncated);
        assert_eq!(response.coverage.start, "2026-09-11T10:00:00+00:00");
    }

    #[tokio::test]
    async fn intervals_without_a_summary_are_not_served() {
        let (db, _dir) = test_db().await;
        seed_interval(
            &db,
            "fixture",
            "task-1",
            "已有摘要的工作",
            "interval-summarized",
            "2026-09-11T10:00:00Z",
            "2026-09-11T10:15:00Z",
            &["2026-09-11T10:05:00Z"],
            Some(("已生成摘要。", "[]", "short")),
        )
        .await;
        seed_interval(
            &db,
            "fixture",
            "task-2",
            "尚无摘要的工作",
            "interval-pending",
            "2026-09-11T10:20:00Z",
            "2026-09-11T10:35:00Z",
            &["2026-09-11T10:25:00Z"],
            None,
        )
        .await;

        let response = load_interval_summaries(
            &db,
            at("2026-09-11T10:00:00Z"),
            at("2026-09-11T11:00:00Z"),
            DEFAULT_SUMMARY_LIMIT,
        )
        .await
        .unwrap();

        assert_eq!(entry_ids(&response), ["work:1"]);
    }

    #[tokio::test]
    async fn page_boundaries_trim_intervals_that_merely_overlap() {
        let (db, _dir) = test_db().await;
        // Ends at 10:15 but starts before the page: excluded.
        seed_interval(
            &db,
            "fixture",
            "task-1",
            "早前页的工作",
            "interval-early",
            "2026-09-11T09:45:00Z",
            "2026-09-11T10:15:00Z",
            &["2026-09-11T09:50:00Z"],
            Some(("属于更早一页。", "[]", "short")),
        )
        .await;
        // Starts exactly at the page start: included.
        seed_interval(
            &db,
            "fixture",
            "task-2",
            "页首的工作",
            "interval-first",
            "2026-09-11T10:00:00Z",
            "2026-09-11T10:15:00Z",
            &["2026-09-11T10:05:00Z"],
            Some(("页首摘要。", "[]", "short")),
        )
        .await;
        // Starts exactly at the page end: excluded.
        seed_interval(
            &db,
            "fixture",
            "task-3",
            "下一页的工作",
            "interval-next",
            "2026-09-11T11:00:00Z",
            "2026-09-11T11:15:00Z",
            &["2026-09-11T11:05:00Z"],
            Some(("属于下一页。", "[]", "short")),
        )
        .await;

        let response = load_interval_summaries(
            &db,
            at("2026-09-11T10:00:00Z"),
            at("2026-09-11T11:00:00Z"),
            DEFAULT_SUMMARY_LIMIT,
        )
        .await
        .unwrap();

        assert_eq!(entry_ids(&response), ["work:2"]);
    }

    #[tokio::test]
    async fn limit_marks_truncation_on_the_response_coverage() {
        let (db, _dir) = test_db().await;
        for index in 0..3u32 {
            let minute = index * 10;
            let start = format!("2026-09-11T10:{minute:02}:00Z");
            let end = format!("2026-09-11T10:{:02}:00Z", minute + 9);
            seed_interval(
                &db,
                "fixture",
                &format!("task-{index}"),
                &format!("工作{index}"),
                &format!("interval-{index}"),
                &start,
                &end,
                &[&start],
                Some((format!("摘要{index}。").as_str(), "[]", "short")),
            )
            .await;
        }

        let full = load_interval_summaries(
            &db,
            at("2026-09-11T10:00:00Z"),
            at("2026-09-11T11:00:00Z"),
            DEFAULT_SUMMARY_LIMIT,
        )
        .await
        .unwrap();
        assert_eq!(full.entries.len(), 3);
        assert!(!full.coverage.truncated);

        let truncated = load_interval_summaries(
            &db,
            at("2026-09-11T10:00:00Z"),
            at("2026-09-11T11:00:00Z"),
            2,
        )
        .await
        .unwrap();
        assert_eq!(entry_ids(&truncated), ["work:1", "work:2"]);
        assert!(truncated.coverage.truncated);
    }

    #[tokio::test]
    async fn meeting_spans_classify_the_entry_kind_and_id() {
        let (db, _dir) = test_db().await;
        // 10:00–10:40 with a meeting covering 10:05–10:35: over half → meeting.
        seed_interval(
            &db,
            "fixture",
            "task-meeting",
            "参加会议",
            "interval-meeting",
            "2026-09-11T10:00:00Z",
            "2026-09-11T10:40:00Z",
            &["2026-09-11T10:10:00Z"],
            Some(("会议摘要。", "[]", "short")),
        )
        .await;
        seed_meeting(
            &db,
            "2026-09-11T10:05:00Z",
            "2026-09-11T10:35:00Z",
        )
        .await;
        // 10:45–11:25 with only a 10-minute meeting overlap: below half → work.
        seed_interval(
            &db,
            "fixture",
            "task-work",
            "会议后的工作",
            "interval-work",
            "2026-09-11T10:45:00Z",
            "2026-09-11T11:25:00Z",
            &["2026-09-11T10:50:00Z"],
            Some(("工作摘要。", "[]", "short")),
        )
        .await;
        seed_meeting(
            &db,
            "2026-09-11T11:00:00Z",
            "2026-09-11T11:10:00Z",
        )
        .await;

        let response = load_interval_summaries(
            &db,
            at("2026-09-11T10:00:00Z"),
            at("2026-09-11T12:00:00Z"),
            DEFAULT_SUMMARY_LIMIT,
        )
        .await
        .unwrap();

        assert_eq!(entry_ids(&response), ["meeting:1", "work:2"]);
        assert_eq!(response.entries[0].kind, "meeting");
        assert_eq!(response.entries[1].kind, "work");
    }

    #[tokio::test]
    async fn empty_database_page_reads_as_an_empty_non_truncated_response() {
        let (db, _dir) = test_db().await;

        let response = load_interval_summaries(
            &db,
            at("2026-09-11T10:00:00Z"),
            at("2026-09-11T11:00:00Z"),
            DEFAULT_SUMMARY_LIMIT,
        )
        .await
        .unwrap();

        assert!(response.entries.is_empty());
        assert!(!response.coverage.truncated);
    }
}
