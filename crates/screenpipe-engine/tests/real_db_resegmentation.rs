// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

#![allow(clippy::type_complexity)]

use chrono::{DateTime, Duration, Utc};
use screenpipe_config::DbConfig;
use screenpipe_db::DatabaseManager;
use screenpipe_engine::activity_ledger::reconcile_range;
use screenpipe_engine::knowledge::{extract, summarize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn test_lock() -> MutexGuard<'static, ()> {
    TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn db_path() -> String {
    std::env::var("SP_REAL_DB_COPY").expect("SP_REAL_DB_COPY must point at the read-only test copy")
}

async fn open_copy() -> DatabaseManager {
    let path = db_path();
    assert!(Path::new(&path).is_file(), "missing database copy: {path}");
    DatabaseManager::new(&path, DbConfig::default())
        .await
        .unwrap_or_else(|e| panic!("DatabaseManager could not open copy: {e}"))
}

async fn window(db: &DatabaseManager) -> (DateTime<Utc>, DateTime<Utc>) {
    let row = sqlx::query("SELECT MAX(end_at) AS max_end FROM activity_intervals")
        .fetch_one(&db.pool)
        .await
        .expect("max interval end query");
    let max_end: Option<String> = row.try_get("max_end").unwrap();
    let end = max_end
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|t| t.with_timezone(&Utc))
        .expect("real database has activity intervals");
    (end - Duration::hours(6), end)
}

async fn count(db: &DatabaseManager, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(&db.pool).await.unwrap()
}

async fn report_line(line: impl AsRef<str>) {
    println!("R1 {}", line.as_ref());
}

#[tokio::test]
#[ignore]
async fn real_resegmentation() {
    let _lock = test_lock();
    let db = open_copy().await;
    let migration_count = count(&db, "SELECT COUNT(*) FROM _sqlx_migrations").await;
    assert!(migration_count >= 126, "expected migrations to be applied");
    let m1: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 20260912100000")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let m2: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 20260912102000")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!((m1, m2), (1, 1));

    let intervals_before = count(&db, "SELECT COUNT(*) FROM activity_intervals").await;
    let jobs_before = count(&db, "SELECT COUNT(*) FROM knowledge_jobs").await;
    let coverage = count(&db, "SELECT COUNT(*) FROM activity_ledger_coverage").await;
    let active_before = count(&db, "SELECT COUNT(*) FROM activity_intervals_active").await;
    if coverage == 0 {
        assert_eq!(
            active_before, intervals_before,
            "no coverage must show all rows"
        );
    }
    let (start, end) = window(&db).await;
    let old_v1: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM activity_intervals WHERE producer='deterministic-v1' AND end_at > ?1 AND start_at < ?2",
    )
    .bind(start.to_rfc3339())
    .bind(end.to_rfc3339())
    .fetch_one(&db.pool)
    .await
    .unwrap();

    let started = Instant::now();
    reconcile_range(&db, start, end)
        .await
        .expect("real v2 reconcile");
    let elapsed_ms = started.elapsed().as_millis();
    let active = db
        .list_activity_ledger(start, end, true, true)
        .await
        .unwrap();
    assert!(
        !active.is_empty(),
        "bounded real window produced no active intervals"
    );
    let v2_count = active
        .iter()
        .filter(|i| i.producer == "deterministic-v2")
        .count();
    assert_eq!(
        v2_count,
        active.len(),
        "active window must expose one version"
    );
    let durations: Vec<i64> = active
        .iter()
        .filter_map(|i| {
            Some(
                (DateTime::parse_from_rfc3339(&i.end_at).ok()?
                    - DateTime::parse_from_rfc3339(&i.start_at).ok()?)
                .num_seconds(),
            )
        })
        .collect();
    let mut sorted = durations.clone();
    sorted.sort_unstable();
    let pct = |p: usize| sorted[(sorted.len() - 1) * p / 100];
    let short = sorted.iter().filter(|s| **s < 60).count();
    let identities: std::collections::BTreeSet<_> = active
        .iter()
        .map(|i| format!("{}|{}", i.app_name.as_deref().unwrap_or(""), i.title))
        .collect();
    let mut hasher = Sha256::new();
    for identity in &identities {
        hasher.update(identity.as_bytes());
    }
    let hash = format!("{:x}", hasher.finalize());
    let v1_total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM activity_intervals WHERE producer='deterministic-v1' AND end_at > ?1 AND start_at < ?2",
    )
    .bind(start.to_rfc3339())
    .bind(end.to_rfc3339())
    .fetch_one(&db.pool)
    .await
    .unwrap();
    report_line(format!(
        "window_hours=6 observations={} v1_segments={} v2_segments={} v1_vs_v2_same_window={}:{} p50_s={} p90_s={} lt60_pct={:.2} identities={} identity_hash={}&elapsed_ms={}",
        active.iter().map(|i| i.evidence_count).sum::<i64>(), old_v1, v2_count, v1_total, v2_count,
        pct(50), pct(90), short as f64 / sorted.len() as f64, identities.len(), &hash[..8], elapsed_ms
    )).await;
    // Reconciliation is expected to add a historical v2 generation; the
    // migration/open invariant was checked before the write above.
    assert_eq!(
        jobs_before,
        count(&db, "SELECT COUNT(*) FROM knowledge_jobs").await
    );
    db.close().await;
}

#[tokio::test]
#[ignore]
async fn invariants() {
    let _lock = test_lock();
    let db = Arc::new(open_copy().await);
    let (start, end) = window(&db).await;
    let effective_1 = db
        .activity_ledger_expand_range_to_active_bounds(start, end, "deterministic-v2")
        .await
        .unwrap();
    report_line(format!("idempotence effective_1=[{}, {})", effective_1.0, effective_1.1)).await;
    let historical_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM activity_intervals WHERE producer='deterministic-v1' AND end_at > ?1 AND start_at < ?2 ORDER BY id LIMIT 1",
    ).bind(start.to_rfc3339()).bind(end.to_rfc3339()).fetch_optional(&db.pool).await.unwrap();
    reconcile_range(&db, start, end).await.unwrap();
    let rows = db
        .list_activity_ledger(start, end, true, true)
        .await
        .unwrap();
    for pair in rows.windows(2) {
        assert!(
            pair[0].end_at <= pair[1].start_at,
            "active intervals overlap"
        );
    }
    // The invariant applies to v2 output only. Historical v1 rows remain
    // queryable and are measured separately by `attribution`; they are not
    // rewritten by this fix.
    let bad_evidence: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM activity_evidence e JOIN activity_intervals_active i ON i.id=e.interval_id WHERE i.producer='deterministic-v2' AND (e.occurred_at < i.start_at OR e.occurred_at >= i.end_at)",
    ).fetch_one(&db.pool).await.unwrap();
    let orphan: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM activity_evidence e LEFT JOIN activity_intervals i ON i.id=e.interval_id WHERE i.id IS NULL",
    ).fetch_one(&db.pool).await.unwrap();
    report_line(format!(
        "invariants_precheck intervals={} bad_evidence={} orphan_evidence={}",
        rows.len(),
        bad_evidence,
        orphan
    ))
    .await;
    assert_eq!(bad_evidence, 0);
    assert_eq!(orphan, 0);
    let first_id = historical_id;
    let keys_before: std::collections::BTreeSet<String> = sqlx::query_scalar(
        "SELECT interval_key FROM activity_intervals_active WHERE producer='deterministic-v2' AND end_at > ?1 AND start_at < ?2 ORDER BY interval_key",
    )
    .bind(start.to_rfc3339()).bind(end.to_rfc3339()).fetch_all(&db.pool).await.unwrap().into_iter().collect();
    let first_dispatched = extract::discover_and_enqueue(&db, start).await.unwrap();
    let hashes_before: std::collections::BTreeSet<String> = sqlx::query_scalar(
        "SELECT DISTINCT input_hash FROM knowledge_jobs WHERE kind='extract' AND input_hash IS NOT NULL",
    ).fetch_all(&db.pool).await.unwrap().into_iter().collect();
    let effective_2 = db
        .activity_ledger_expand_range_to_active_bounds(start, end, "deterministic-v2")
        .await
        .unwrap();
    report_line(format!("idempotence effective_2=[{}, {})", effective_2.0, effective_2.1)).await;
    reconcile_range(&db, start, end).await.unwrap();
    let keys_after: std::collections::BTreeSet<String> = sqlx::query_scalar(
        "SELECT interval_key FROM activity_intervals_active WHERE producer='deterministic-v2' AND end_at > ?1 AND start_at < ?2 ORDER BY interval_key",
    )
    .bind(start.to_rfc3339()).bind(end.to_rfc3339()).fetch_all(&db.pool).await.unwrap().into_iter().collect();
    let second_dispatched = extract::discover_and_enqueue(&db, start).await.unwrap();
    let hashes_after: std::collections::BTreeSet<String> = sqlx::query_scalar(
        "SELECT DISTINCT input_hash FROM knowledge_jobs WHERE kind='extract' AND input_hash IS NOT NULL",
    ).fetch_all(&db.pool).await.unwrap().into_iter().collect();
    report_line(format!("idempotence keys_equal={} before={} after={} hashes_equal={} first_dispatch={} second_dispatch={}", keys_before == keys_after, keys_before.len(), keys_after.len(), hashes_before == hashes_after, first_dispatched, second_dispatched)).await;
    assert_eq!(keys_before, keys_after, "interval key set changed");
    assert_eq!(hashes_before, hashes_after, "discovery input hash set changed");
    assert_eq!(second_dispatched, 0, "same window created new extract jobs");
    let rows_again = db
        .list_activity_ledger(start, end, true, true)
        .await
        .unwrap();
    if let Some(id) = first_id {
        let found: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_intervals WHERE id=?1")
            .bind(id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(found, 1, "historical interval is not queryable by id");
    }
    let active_versions: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT producer) FROM activity_intervals_active WHERE end_at > ?1 AND start_at < ?2",
    ).bind(start.to_rfc3339()).bind(end.to_rfc3339()).fetch_one(&db.pool).await.unwrap();
    assert_eq!(active_versions, 1);
    report_line(format!("invariants intervals={} bad_evidence={} orphan_evidence={} active_versions={} idempotent=true", rows.len(), bad_evidence, orphan, active_versions)).await;
    db.close().await;
}

#[derive(Debug)]
struct BoundaryStat {
    producer: String,
    direction: String,
    magnitude: String,
}

fn magnitude(seconds: i64) -> &'static str {
    match seconds.abs() {
        0 => "boundary_0",
        1..=60 => "1_60s",
        61..=300 => "61_300s",
        301..=3600 => "301_3600s",
        _ => "gt_3600s",
    }
}

async fn boundary_stats(
    db: &DatabaseManager,
    producer: &str,
    start: &DateTime<Utc>,
    end: &DateTime<Utc>,
) -> Vec<BoundaryStat> {
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT i.producer, i.start_at, i.end_at, e.occurred_at \
         FROM activity_evidence e JOIN activity_intervals i ON i.id=e.interval_id \
         WHERE i.producer=?1 AND i.end_at>?2 AND i.start_at<?3",
    )
    .bind(producer)
    .bind(start.to_rfc3339())
    .bind(end.to_rfc3339())
    .fetch_all(&db.pool)
    .await
    .unwrap();
    rows.into_iter()
        .filter_map(|(producer, interval_start, interval_end, occurred)| {
            let s = DateTime::parse_from_rfc3339(&interval_start)
                .ok()?
                .with_timezone(&Utc);
            let e = DateTime::parse_from_rfc3339(&interval_end)
                .ok()?
                .with_timezone(&Utc);
            let at = DateTime::parse_from_rfc3339(&occurred)
                .ok()?
                .with_timezone(&Utc);
            let (direction, delta) = if at < s {
                ("early", (s - at).num_seconds())
            } else if at >= e {
                ("late", (at - e).num_seconds())
            } else {
                return None;
            };
            Some(BoundaryStat {
                producer,
                direction: direction.to_string(),
                magnitude: magnitude(delta).to_string(),
            })
        })
        .collect()
}

fn summarize_boundaries(stats: &[BoundaryStat]) -> String {
    let mut counts = std::collections::BTreeMap::<(String, String), usize>::new();
    for stat in stats {
        *counts
            .entry((stat.direction.clone(), stat.magnitude.clone()))
            .or_default() += 1;
    }
    counts
        .into_iter()
        .map(|((d, m), n)| format!("{d}:{m}={n}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[tokio::test]
#[ignore]
async fn attribution() {
    let _lock = test_lock();
    let db = Arc::new(open_copy().await);
    let (start, end) = window(&db).await;
    let v1_stats = boundary_stats(&db, "deterministic-v1", &start, &end).await;
    reconcile_range(&db, start, end).await.unwrap();
    let v2_stats = boundary_stats(&db, "deterministic-v2", &start, &end).await;
    report_line(format!(
        "attribution v1_bad={} [{}] v2_bad={} [{}] total_bad={} window_hours=6",
        v1_stats.len(),
        summarize_boundaries(&v1_stats),
        v2_stats.len(),
        summarize_boundaries(&v2_stats),
        v1_stats.len() + v2_stats.len()
    ))
    .await;

    let anchor_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT CASE WHEN EXISTS (SELECT 1 FROM activity_evidence e JOIN frames f ON e.source_type='frame' AND e.source_id=f.id WHERE e.interval_id=i.id AND f.document_path IS NOT NULL AND f.document_path!='') THEN 'document_path' \
         WHEN EXISTS (SELECT 1 FROM activity_evidence e JOIN frames f ON e.source_type='frame' AND e.source_id=f.id WHERE e.interval_id=i.id AND f.semantic_run_id IS NOT NULL) \
           OR EXISTS (SELECT 1 FROM activity_evidence e JOIN ui_events u ON e.source_type='ui_event' AND e.source_id=u.id WHERE e.interval_id=i.id AND (u.element_automation_id IS NOT NULL OR u.element_name IS NOT NULL)) THEN 'semantic' \
         WHEN EXISTS (SELECT 1 FROM activity_evidence e LEFT JOIN frames f ON e.source_type='frame' AND e.source_id=f.id LEFT JOIN ui_events u ON e.source_type='ui_event' AND e.source_id=u.id WHERE e.interval_id=i.id AND COALESCE(f.browser_url,u.browser_url) IS NOT NULL AND COALESCE(f.browser_url,u.browser_url)!='') THEN 'site' ELSE 'app_fallback' END AS anchor, COUNT(*) \
         FROM activity_intervals i WHERE i.producer='deterministic-v2' AND i.end_at>?1 AND i.start_at<?2 GROUP BY anchor ORDER BY anchor",
    ).bind(start.to_rfc3339()).bind(end.to_rfc3339()).fetch_all(&db.pool).await.unwrap();
    let anchors = anchor_rows
        .iter()
        .map(|(a, n)| format!("{a}={n}"))
        .collect::<Vec<_>>()
        .join(",");

    let identity_rows: Vec<(i64, i64)> = sqlx::query_as("SELECT task_id, COUNT(*) FROM activity_intervals WHERE producer='deterministic-v2' AND end_at>?1 AND start_at<?2 GROUP BY task_id ORDER BY task_id").bind(start.to_rfc3339()).bind(end.to_rfc3339()).fetch_all(&db.pool).await.unwrap();
    let mut hist = std::collections::BTreeMap::<i64, i64>::new();
    for (_, n) in &identity_rows {
        *hist.entry(*n).or_default() += 1;
    }
    let histogram = hist
        .into_iter()
        .map(|(n, c)| format!("{n}:{c}"))
        .collect::<Vec<_>>()
        .join(",");
    let transition_rows: Vec<(i64, String, String, i64)> = sqlx::query_as("SELECT task_id,start_at,end_at,id FROM activity_intervals WHERE producer='deterministic-v2' AND end_at>?1 AND start_at<?2 ORDER BY start_at,id").bind(start.to_rfc3339()).bind(end.to_rfc3339()).fetch_all(&db.pool).await.unwrap();
    let mut transitions = std::collections::BTreeMap::<&str, usize>::new();
    for pair in transition_rows.windows(2) {
        let (a_task, _, a_end, _) = &pair[0];
        let (b_task, b_start, _, _) = &pair[1];
        let a_end = DateTime::parse_from_rfc3339(a_end).unwrap();
        let b_start = DateTime::parse_from_rfc3339(b_start).unwrap();
        let kind = if b_start > a_end {
            "gap"
        } else if a_task == b_task {
            "same_identity_reentry"
        } else {
            "cross_identity"
        };
        *transitions.entry(kind).or_default() += 1;
    }
    let transition_text = transitions
        .into_iter()
        .map(|(k, n)| format!("{k}={n}"))
        .collect::<Vec<_>>()
        .join(",");
    report_line(format!(
        "fragmentation v2_intervals={} anchors=[{}] identity_hist=[{}] adjacent=[{}]",
        transition_rows.len(),
        anchors,
        histogram,
        transition_text
    ))
    .await;

    let before_id: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id),0) FROM knowledge_jobs")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let terminal: Vec<(String, Option<String>, Option<String>, i64)> = sqlx::query_as("SELECT scope_key,input_hash,payload,model_calls FROM knowledge_jobs WHERE kind='extract' AND state IN ('succeeded','failed')").fetch_all(&db.pool).await.unwrap();
    let candidates = extract::discover_and_enqueue(&db, start).await.unwrap();
    let pending: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT scope_key,input_hash,payload FROM knowledge_jobs WHERE id>?1 AND kind='extract'",
    )
    .bind(before_id)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    let mut repeated = 0usize;
    let mut fresh = 0usize;
    let mut consumed_calls = 0i64;
    for (scope, hash, payload) in &pending {
        let p: Value = payload
            .as_deref()
            .and_then(|v| serde_json::from_str(v).ok())
            .unwrap_or(Value::Null);
        let ps = p.get("interval_start").and_then(Value::as_str);
        let pe = p.get("interval_end").and_then(Value::as_str);
        let overlaps = terminal
            .iter()
            .filter(|(old_scope, old_hash, old_payload, calls)| {
                if old_scope != scope {
                    return false;
                }
                if old_hash == hash {
                    return true;
                }
                let q: Value = old_payload
                    .as_deref()
                    .and_then(|v| serde_json::from_str(v).ok())
                    .unwrap_or(Value::Null);
                match (
                    ps,
                    pe,
                    q.get("interval_start").and_then(Value::as_str),
                    q.get("interval_end").and_then(Value::as_str),
                ) {
                    (Some(a), Some(b), Some(c), Some(d)) => a < d && c < b,
                    _ => false,
                }
            })
            .collect::<Vec<_>>();
        if overlaps.is_empty() {
            fresh += 1;
        } else {
            repeated += 1;
            consumed_calls += overlaps.iter().map(|(_, _, _, calls)| *calls).sum::<i64>();
        }
    }
    report_line(format!("duplicate_consumption extract_candidates={} newly_enqueued={} repeated={} non_repeated={} prior_terminal_model_calls_overlap_sum={} model_calls_estimate=terminal_model_calls_sum", candidates, pending.len(), repeated, fresh, consumed_calls)).await;
    db.close().await;
}

#[tokio::test]
#[ignore]
async fn dryrun() {
    let _lock = test_lock();
    let db = Arc::new(open_copy().await);
    let before_terminal: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM knowledge_jobs WHERE state IN ('failed','succeeded')",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let before_jobs = count(&db, "SELECT COUNT(*) FROM knowledge_jobs").await;
    let (start, _) = window(&db).await;
    let summarize_candidates = summarize::discover_and_enqueue(&db, start).await.unwrap();
    let extract_candidates = extract::discover_and_enqueue(&db, start).await.unwrap();
    let after_terminal: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM knowledge_jobs WHERE state IN ('failed','succeeded')",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let after_jobs = count(&db, "SELECT COUNT(*) FROM knowledge_jobs").await;
    assert_eq!(
        before_terminal, after_terminal,
        "terminal identities changed during discovery"
    );
    report_line(format!("dryrun summarize_candidates={} extract_candidates={} terminal_before={} terminal_after={} model_calls=0 dedup_intercepts={} new_dispatches={}", summarize_candidates, extract_candidates, before_terminal, after_terminal, before_jobs - after_jobs + summarize_candidates as i64 + extract_candidates as i64, after_jobs - before_jobs)).await;
    db.close().await;
}
