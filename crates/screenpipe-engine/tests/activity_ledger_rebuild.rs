// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! ① 活动间隔重建 tick 的引擎层验收（plan §4.2.1 / §4.3.1）：在内存库上证明
//! `reconcile_range_with_policy` 有可观测产出（重建出活跃间隔）、同身份小间隙
//! 保持连续而跨身份切换独立成段、同窗重复重建幂等（间隔键集合稳定）。
//! policy 阈值本身（merge_gap/min_dwell）对切分的影响由 crate 内
//! `fold_short_same_identity_segments` 单元测试覆盖；设置→policy 的映射由
//! `cadence.rs` 测试覆盖。legacy 自动叙事停用后，间隔数据流由这条路径独立保证。

use chrono::{DateTime, Utc};
use screenpipe_config::DbConfig;
use screenpipe_db::DatabaseManager;
use screenpipe_engine::activity_ledger::{reconcile_range_with_policy, SegmentationPolicy};

async fn test_db() -> DatabaseManager {
    DatabaseManager::new("sqlite::memory:", DbConfig::default())
        .await
        .unwrap()
}

/// Two ui_event observations with the same identity (app + window), separated
/// by an unobserved gap of `gap_minutes`.
async fn seed_same_identity_pair(db: &DatabaseManager, gap_minutes: i64) {
    let mut tx = db.begin_immediate_with_retry().await.unwrap();
    for k in 0..2 {
        let minute = if k == 0 { 0 } else { 4 + gap_minutes };
        let at = format!("2026-09-12T09:{minute:02}:00Z");
        sqlx::query(
            "INSERT INTO ui_events \
             (timestamp, relative_ms, event_type, app_name, window_title, element_value) \
             VALUES (?1, 0, 'click', 'Arc', 'Ticket 42', ?2)",
        )
        .bind(&at)
        .bind(format!("clicked-{k}"))
        .execute(&mut **tx.conn())
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

async fn active_interval_keys(db: &DatabaseManager) -> Vec<String> {
    let mut keys: Vec<String> = sqlx::query_scalar(
        "SELECT interval_key FROM activity_intervals_active ORDER BY interval_key",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    keys.sort();
    keys
}

fn at(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

#[tokio::test]
async fn rebuild_tick_produces_active_intervals() {
    let db = test_db().await;
    seed_same_identity_pair(&db, 1).await;
    let start = at("2026-09-12T09:00:00Z");
    let end = at("2026-09-12T09:10:00Z");

    let intervals = reconcile_range_with_policy(
        &db,
        start,
        end,
        SegmentationPolicy::default(),
    )
    .await
    .unwrap();
    assert!(intervals > 0, "① tick 必须有可观测产出（活跃间隔）");
    assert_eq!(
        intervals,
        active_interval_keys(&db).await.len(),
        "返回值与活跃间隔表一致"
    );
}

#[tokio::test]
async fn rebuild_keeps_same_object_continuity_and_splits_identities() {
    let db = test_db().await;
    // 同身份、3 分钟未观测间隙（≤ UNOBSERVED_GAP）：回到刚才那件事 → 并为一段。
    seed_same_identity_pair(&db, 3).await;
    // 再造一个不同身份的观察（别的 app）：必须独立成段。
    {
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        sqlx::query(
            "INSERT INTO ui_events \
             (timestamp, relative_ms, event_type, app_name, window_title, element_value) \
             VALUES ('2026-09-12T09:20:00Z', 0, 'click', 'WeChat', 'Chat', 'msg')",
        )
        .execute(&mut **tx.conn())
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    let start = at("2026-09-12T09:00:00Z");
    let end = at("2026-09-12T09:30:00Z");

    let intervals =
        reconcile_range_with_policy(&db, start, end, SegmentationPolicy::default())
            .await
            .unwrap();
    let keys = active_interval_keys(&db).await;
    assert_eq!(intervals, keys.len());
    assert!(keys.len() >= 2, "跨身份切换必须独立成段：{keys:?}");
}

#[tokio::test]
async fn repeated_rebuild_of_the_same_window_is_idempotent() {
    let db = test_db().await;
    seed_same_identity_pair(&db, 1).await;
    let start = at("2026-09-12T09:00:00Z");
    let end = at("2026-09-12T09:10:00Z");

    reconcile_range_with_policy(&db, start, end, SegmentationPolicy::default())
        .await
        .unwrap();
    let first = active_interval_keys(&db).await;
    reconcile_range_with_policy(&db, start, end, SegmentationPolicy::default())
        .await
        .unwrap();
    let second = active_interval_keys(&db).await;
    assert_eq!(first, second, "同窗重复重建：interval_key 集合稳定");
}

/// 回归（R2 验收阻塞）：同一毫秒内的两条观察被 SQL 毫秒 ROUND 塌缩成同一
/// 时刻后，跨身份边界观察会把前一段的 end_at 推到与其尾部 evidence 相同的
/// 时刻，违反半开区间 [start_at, end_at)。微秒精度必须一路保留到 evidence。
#[tokio::test]
async fn evidence_stays_strictly_before_the_interval_end_within_one_millisecond() {
    let db = test_db().await;
    {
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        for (app, ts, value) in [
            ("Editor", "2026-09-12T09:00:00.100000Z", "first"),
            // 与下一行（身份边界观察）同处一毫秒：毫秒塌缩曾让这条尾部
            // evidence 的 occurred_at 进位到前一段的 end_at 上。
            ("Editor", "2026-09-12T09:00:30.100620Z", "tail"),
            ("Terminal", "2026-09-12T09:00:30.100650Z", "next"),
        ] {
            sqlx::query(
                "INSERT INTO ui_events \
                 (timestamp, relative_ms, event_type, app_name, window_title, element_value) \
                 VALUES (?1, 0, 'click', ?2, 'window', ?3)",
            )
            .bind(ts)
            .bind(app)
            .bind(value)
            .execute(&mut **tx.conn())
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
    }

    let start = at("2026-09-12T09:00:00Z");
    let end = at("2026-09-12T09:01:00Z");
    reconcile_range_with_policy(&db, start, end, SegmentationPolicy::default())
        .await
        .unwrap();

    let rows = db
        .list_activity_ledger(start, end, false, true)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "跨身份边界必须独立成段");
    let parse = |value: &str| DateTime::parse_from_rfc3339(value).unwrap().with_timezone(&Utc);
    let first = &rows[0];
    assert_eq!(first.start_at, "2026-09-12T09:00:00.100+00:00");
    assert_eq!(first.end_at, "2026-09-12T09:00:30.100650+00:00");
    let tail = first.evidence.last().unwrap();
    assert_eq!(
        tail.occurred_at, "2026-09-12T09:00:30.100620+00:00",
        "尾部 evidence 必须保留微秒精度，而不是进位到边界毫秒"
    );
    assert!(
        parse(&tail.occurred_at) < parse(&first.end_at),
        "evidence 必须严格早于区间右边界：{} !< {}",
        tail.occurred_at,
        first.end_at
    );
    let second = &rows[1];
    assert_eq!(second.start_at, "2026-09-12T09:00:30.100650+00:00");
    assert_eq!(second.evidence.len(), 1);
    assert_eq!(second.evidence[0].occurred_at, second.start_at);
}
