// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! 沉淀管线四层节拍与预算配置（plan §4.2）。
//!
//! 13 个设置键全部落 `settings.extra`（自由键），由桌面壳在每次 tick 前读取并
//! 构造 [`KnowledgeCadenceConfig`] 传给引擎；引擎不认识 SettingsStore，只认
//! 这份已校验的数值。每个键都有默认值与允许范围：缺省、类型错误或越界一律
//! 回落到默认/夹紧，用户不改也能跑。
//!
//! 迁移兼容（§4.2.4）：旧键 `activitiesIntervalMinutes` 存在时作为
//! `knowledgeReconcileMinutes` 的初值读取——它语义上是"活动数据处理的
//! 节拍"，legacy 叙事停用后由 ① tick 接管；只读不写，不静默改动用户已有值。

use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

use super::executor::KnowledgeJobLimits;

/// ① 活动间隔重建节拍（分钟）。
pub const KEY_RECONCILE_MINUTES: &str = "knowledgeReconcileMinutes";
/// ① 重建回看窗口（小时）。
pub const KEY_RECONCILE_WINDOW_HOURS: &str = "knowledgeReconcileWindowHours";
/// ② 间隔摘要扫描节拍（分钟）。
pub const KEY_SUMMARIZE_MINUTES: &str = "knowledgeSummarizeMinutes";
/// ③ WorkUnit 抽取扫描节拍（分钟）。
pub const KEY_WORK_UNIT_MINUTES: &str = "knowledgeWorkUnitMinutes";
/// ④ 知识蒸馏周期（小时）。
pub const KEY_DISTILL_HOURS: &str = "knowledgeDistillHours";
/// ④ 同 scope 重新蒸馏冷却期（天）。
pub const KEY_DISTILL_COOLDOWN_DAYS: &str = "knowledgeDistillCooldownDays";
/// ① 相邻段合并阈值（分钟，§4.4）。
pub const KEY_ACTIVITY_MERGE_GAP_MINUTES: &str = "activityMergeGapMinutes";
/// ① 短段吸收阈值（秒，§4.4）。
pub const KEY_ACTIVITY_MIN_DWELL_SECONDS: &str = "activityMinDwellSeconds";
/// ②③ 发现回看窗口（小时）。
pub const KEY_DISCOVERY_LOOKBACK_HOURS: &str = "knowledgeDiscoveryLookbackHours";
/// ②③ 每轮发现投递上限。
pub const KEY_DISCOVERY_BATCH: &str = "knowledgeDiscoveryBatch";
/// 单次模型调用超时（秒）。
pub const KEY_CALL_TIMEOUT_SECONDS: &str = "knowledgeCallTimeoutSeconds";
/// 单任务总时长（秒）。
pub const KEY_STEP_TIMEOUT_SECONDS: &str = "knowledgeStepTimeoutSeconds";
/// 每任务模型调用上限。
pub const KEY_MAX_MODEL_CALLS: &str = "knowledgeMaxModelCallsPerJob";
/// Legacy 键：旧叙事生成间隔（分钟）。仅作 `knowledgeReconcileMinutes` 的
/// 迁移初值来源，UI 上标注为「仅影响旧叙事生成」。
pub const LEGACY_KEY_INTERVAL_MINUTES: &str = "activitiesIntervalMinutes";

/// One configurable quantity: key, default, inclusive allowed range.
struct SettingSpec {
    key: &'static str,
    default: u64,
    min: u64,
    max: u64,
}

impl SettingSpec {
    fn read(&self, extra: &HashMap<String, Value>) -> u64 {
        raw_u64(extra, self.key)
            .map(|v| v.clamp(self.min, self.max))
            .unwrap_or(self.default)
    }
}

const RECONCILE_MINUTES: SettingSpec = SettingSpec { key: KEY_RECONCILE_MINUTES, default: 5, min: 1, max: 60 };
const RECONCILE_WINDOW_HOURS: SettingSpec = SettingSpec { key: KEY_RECONCILE_WINDOW_HOURS, default: 2, min: 1, max: 48 };
const SUMMARIZE_MINUTES: SettingSpec = SettingSpec { key: KEY_SUMMARIZE_MINUTES, default: 15, min: 5, max: 1440 };
const WORK_UNIT_MINUTES: SettingSpec = SettingSpec { key: KEY_WORK_UNIT_MINUTES, default: 60, min: 5, max: 1440 };
const DISTILL_HOURS: SettingSpec = SettingSpec { key: KEY_DISTILL_HOURS, default: 24, min: 1, max: 168 };
const DISTILL_COOLDOWN_DAYS: SettingSpec = SettingSpec { key: KEY_DISTILL_COOLDOWN_DAYS, default: 7, min: 1, max: 90 };
const ACTIVITY_MERGE_GAP_MINUTES: SettingSpec = SettingSpec { key: KEY_ACTIVITY_MERGE_GAP_MINUTES, default: 10, min: 1, max: 120 };
const ACTIVITY_MIN_DWELL_SECONDS: SettingSpec = SettingSpec { key: KEY_ACTIVITY_MIN_DWELL_SECONDS, default: 30, min: 5, max: 300 };
const DISCOVERY_LOOKBACK_HOURS: SettingSpec = SettingSpec { key: KEY_DISCOVERY_LOOKBACK_HOURS, default: 26, min: 1, max: 168 };
const DISCOVERY_BATCH: SettingSpec = SettingSpec { key: KEY_DISCOVERY_BATCH, default: 20, min: 1, max: 500 };
const CALL_TIMEOUT_SECONDS: SettingSpec = SettingSpec { key: KEY_CALL_TIMEOUT_SECONDS, default: 45, min: 10, max: 600 };
const STEP_TIMEOUT_SECONDS: SettingSpec = SettingSpec { key: KEY_STEP_TIMEOUT_SECONDS, default: 120, min: 30, max: 1800 };
const MAX_MODEL_CALLS: SettingSpec = SettingSpec { key: KEY_MAX_MODEL_CALLS, default: 3, min: 1, max: 10 };

/// The four cadence layers, discovery windows, budgets and the §4.4
/// segmentation thresholds, all settings-driven (plan §4.2). The desktop shell
/// rebuilds this before every tick, so a settings change applies on the next
/// cycle without a restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnowledgeCadenceConfig {
    /// ① 活动间隔重建节拍（分钟）。
    pub reconcile_minutes: u64,
    /// ① 重建回看窗口（小时）。
    pub reconcile_window_hours: u64,
    /// ② 间隔摘要扫描节拍（分钟）。
    pub summarize_minutes: u64,
    /// ③ WorkUnit 抽取扫描节拍（分钟）。
    pub work_unit_minutes: u64,
    /// ④ 知识蒸馏周期（小时）。
    pub distill_hours: u64,
    /// ④ 同 scope 重新蒸馏冷却期（天）。
    pub distill_cooldown_days: u64,
    /// ① 相邻段合并阈值（分钟）。
    pub activity_merge_gap_minutes: u64,
    /// ① 短段吸收阈值（秒）。
    pub activity_min_dwell_seconds: u64,
    /// ②③ 发现回看窗口（小时）。
    pub discovery_lookback_hours: u64,
    /// ②③ 每轮发现投递上限。
    pub discovery_batch: u32,
    /// 单次模型调用超时。
    pub call_timeout: Duration,
    /// 单任务总时长。
    pub step_timeout: Duration,
    /// 每任务模型调用上限。
    pub max_model_calls: u32,
}

impl Default for KnowledgeCadenceConfig {
    fn default() -> Self {
        Self {
            reconcile_minutes: RECONCILE_MINUTES.default as u64,
            reconcile_window_hours: RECONCILE_WINDOW_HOURS.default,
            summarize_minutes: SUMMARIZE_MINUTES.default,
            work_unit_minutes: WORK_UNIT_MINUTES.default,
            distill_hours: DISTILL_HOURS.default,
            distill_cooldown_days: DISTILL_COOLDOWN_DAYS.default,
            activity_merge_gap_minutes: ACTIVITY_MERGE_GAP_MINUTES.default,
            activity_min_dwell_seconds: ACTIVITY_MIN_DWELL_SECONDS.default,
            discovery_lookback_hours: DISCOVERY_LOOKBACK_HOURS.default,
            discovery_batch: DISCOVERY_BATCH.default as u32,
            call_timeout: Duration::from_secs(CALL_TIMEOUT_SECONDS.default),
            step_timeout: Duration::from_secs(STEP_TIMEOUT_SECONDS.default),
            max_model_calls: MAX_MODEL_CALLS.default as u32,
        }
    }
}

impl KnowledgeCadenceConfig {
    /// Read the 13 keys from `settings.extra`. Absent, non-numeric and
    /// out-of-range values fall back to the default / the allowed range, so a
    /// hand-edited settings file can never disable a layer or run away with
    /// the model budget.
    pub fn from_extra(extra: &HashMap<String, Value>) -> Self {
        let mut config = Self {
            reconcile_minutes: legacy_aware_reconcile_minutes(extra),
            ..Self::default()
        };
        config.reconcile_window_hours = RECONCILE_WINDOW_HOURS.read(extra);
        config.summarize_minutes = SUMMARIZE_MINUTES.read(extra);
        config.work_unit_minutes = WORK_UNIT_MINUTES.read(extra);
        config.distill_hours = DISTILL_HOURS.read(extra);
        config.distill_cooldown_days = DISTILL_COOLDOWN_DAYS.read(extra);
        config.activity_merge_gap_minutes = ACTIVITY_MERGE_GAP_MINUTES.read(extra);
        config.activity_min_dwell_seconds = ACTIVITY_MIN_DWELL_SECONDS.read(extra);
        config.discovery_lookback_hours = DISCOVERY_LOOKBACK_HOURS.read(extra);
        config.discovery_batch = DISCOVERY_BATCH.read(extra) as u32;
        config.call_timeout = Duration::from_secs(CALL_TIMEOUT_SECONDS.read(extra));
        config.step_timeout = Duration::from_secs(STEP_TIMEOUT_SECONDS.read(extra));
        config.max_model_calls = MAX_MODEL_CALLS.read(extra) as u32;
        config
    }

    /// The per-job execution budgets derived from the settings keys.
    pub fn job_limits(&self) -> KnowledgeJobLimits {
        KnowledgeJobLimits {
            call_timeout: self.call_timeout,
            step_timeout: self.step_timeout,
            max_model_calls: self.max_model_calls,
        }
    }

    /// ②③ per-round discovery delivery limits.
    pub fn discovery_batch(&self) -> usize {
        self.discovery_batch as usize
    }
}

/// `knowledgeReconcileMinutes`, falling back to the legacy
/// `activitiesIntervalMinutes` as the migration initial value (§4.2.4): read
/// only — the legacy key is never rewritten or removed.
fn legacy_aware_reconcile_minutes(extra: &HashMap<String, Value>) -> u64 {
    if let Some(value) = raw_u64(extra, KEY_RECONCILE_MINUTES) {
        return value.clamp(RECONCILE_MINUTES.min, RECONCILE_MINUTES.max);
    }
    if let Some(legacy) = raw_u64(extra, LEGACY_KEY_INTERVAL_MINUTES) {
        return legacy.clamp(RECONCILE_MINUTES.min, RECONCILE_MINUTES.max);
    }
    RECONCILE_MINUTES.default
}

/// Numeric extraction: integers and floats (truncated) are accepted; bools,
/// strings and null fall back to the default.
fn raw_u64(extra: &HashMap<String, Value>, key: &str) -> Option<u64> {
    match extra.get(key)? {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f as u64)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn extra(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn defaults_match_plan_4_2_table() {
        let config = KnowledgeCadenceConfig::default();
        assert_eq!(config.reconcile_minutes, 5);
        assert_eq!(config.reconcile_window_hours, 2);
        assert_eq!(config.summarize_minutes, 15);
        assert_eq!(config.work_unit_minutes, 60);
        assert_eq!(config.distill_hours, 24);
        assert_eq!(config.distill_cooldown_days, 7);
        assert_eq!(config.activity_merge_gap_minutes, 10);
        assert_eq!(config.activity_min_dwell_seconds, 30);
        assert_eq!(config.discovery_lookback_hours, 26);
        assert_eq!(config.discovery_batch, 20);
        assert_eq!(config.call_timeout, Duration::from_secs(45));
        assert_eq!(config.step_timeout, Duration::from_secs(120));
        assert_eq!(config.max_model_calls, 3);
    }

    #[test]
    fn every_key_is_read_from_extra() {
        let config = KnowledgeCadenceConfig::from_extra(&extra(&[
            (KEY_RECONCILE_MINUTES, json!(10)),
            (KEY_RECONCILE_WINDOW_HOURS, json!(6)),
            (KEY_SUMMARIZE_MINUTES, json!(30)),
            (KEY_WORK_UNIT_MINUTES, json!(90)),
            (KEY_DISTILL_HOURS, json!(48)),
            (KEY_DISTILL_COOLDOWN_DAYS, json!(14)),
            (KEY_ACTIVITY_MERGE_GAP_MINUTES, json!(20)),
            (KEY_ACTIVITY_MIN_DWELL_SECONDS, json!(60)),
            (KEY_DISCOVERY_LOOKBACK_HOURS, json!(72)),
            (KEY_DISCOVERY_BATCH, json!(50)),
            (KEY_CALL_TIMEOUT_SECONDS, json!(90)),
            (KEY_STEP_TIMEOUT_SECONDS, json!(300)),
            (KEY_MAX_MODEL_CALLS, json!(5)),
        ]));
        assert_eq!(config.reconcile_minutes, 10);
        assert_eq!(config.reconcile_window_hours, 6);
        assert_eq!(config.summarize_minutes, 30);
        assert_eq!(config.work_unit_minutes, 90);
        assert_eq!(config.distill_hours, 48);
        assert_eq!(config.distill_cooldown_days, 14);
        assert_eq!(config.activity_merge_gap_minutes, 20);
        assert_eq!(config.activity_min_dwell_seconds, 60);
        assert_eq!(config.discovery_lookback_hours, 72);
        assert_eq!(config.discovery_batch, 50);
        assert_eq!(config.call_timeout, Duration::from_secs(90));
        assert_eq!(config.step_timeout, Duration::from_secs(300));
        assert_eq!(config.max_model_calls, 5);
    }

    #[test]
    fn out_of_range_values_are_clamped_not_rejected_silently() {
        let config = KnowledgeCadenceConfig::from_extra(&extra(&[
            (KEY_RECONCILE_MINUTES, json!(0)),
            (KEY_RECONCILE_WINDOW_HOURS, json!(999)),
            (KEY_SUMMARIZE_MINUTES, json!(1)),
            (KEY_DISCOVERY_BATCH, json!(100_000)),
            (KEY_CALL_TIMEOUT_SECONDS, json!(2)),
            (KEY_STEP_TIMEOUT_SECONDS, json!(86_400)),
            (KEY_MAX_MODEL_CALLS, json!(0)),
        ]));
        assert_eq!(config.reconcile_minutes, 1, "below min clamps to min");
        assert_eq!(config.reconcile_window_hours, 48, "above max clamps to max");
        assert_eq!(config.summarize_minutes, 5);
        assert_eq!(config.discovery_batch, 500);
        assert_eq!(config.call_timeout, Duration::from_secs(10));
        assert_eq!(config.step_timeout, Duration::from_secs(1800));
        assert_eq!(config.max_model_calls, 1);
    }

    #[test]
    fn invalid_types_fall_back_to_defaults() {
        let config = KnowledgeCadenceConfig::from_extra(&extra(&[
            (KEY_RECONCILE_MINUTES, json!("soon")),
            (KEY_SUMMARIZE_MINUTES, json!(true)),
            (KEY_DISTILL_HOURS, json!(null)),
            (KEY_DISCOVERY_BATCH, json!(1.5)),
        ]));
        assert_eq!(config.reconcile_minutes, 5);
        assert_eq!(config.summarize_minutes, 15);
        assert_eq!(config.distill_hours, 24);
        assert_eq!(config.discovery_batch, 1, "integral floats are accepted");
    }

    #[test]
    fn legacy_interval_minutes_seeds_reconcile_cadence_read_only() {
        // 迁移兼容：旧键存在且新键缺席 → 旧键作为 ① tick 初值（夹紧到范围）。
        let config = KnowledgeCadenceConfig::from_extra(&extra(&[(
            LEGACY_KEY_INTERVAL_MINUTES,
            json!(15),
        )]));
        assert_eq!(config.reconcile_minutes, 15);
        let clamped = KnowledgeCadenceConfig::from_extra(&extra(&[(
            LEGACY_KEY_INTERVAL_MINUTES,
            json!(720),
        )]));
        assert_eq!(clamped.reconcile_minutes, 60, "legacy value clamps into 1–60");

        // 新键优先；两者并存时旧键被忽略，用户的显式新配置不被覆盖。
        let both = KnowledgeCadenceConfig::from_extra(&extra(&[
            (KEY_RECONCILE_MINUTES, json!(5)),
            (LEGACY_KEY_INTERVAL_MINUTES, json!(30)),
        ]));
        assert_eq!(both.reconcile_minutes, 5);
    }

    #[test]
    fn job_limits_carry_the_budget_keys() {
        let config = KnowledgeCadenceConfig::from_extra(&extra(&[
            (KEY_CALL_TIMEOUT_SECONDS, json!(120)),
            (KEY_STEP_TIMEOUT_SECONDS, json!(600)),
            (KEY_MAX_MODEL_CALLS, json!(7)),
        ]));
        let limits = config.job_limits();
        assert_eq!(limits.call_timeout, Duration::from_secs(120));
        assert_eq!(limits.step_timeout, Duration::from_secs(600));
        assert_eq!(limits.max_model_calls, 7);
    }
}
