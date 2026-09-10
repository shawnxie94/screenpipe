# 活动层收口 B01 交付报告（证据按变化保留 + 丢弃记账 + 摘要落库）

执行代理：ZCode；日期：2026-09-11（含纠偏轮）；计划：`docs/plans/activity-layer-retention-execution-plan.md`
（以纠偏提交 `8fc45ce32` 更新后的 §4 为准）
所有验收命令均实际执行；未跑的测试未声称通过。

## 0. 纠偏轮改动摘要

首轮实现把变化键错误地定为 `(app_name, window_title)`——该键由任务身份派生、段内几乎恒定，规则退化成旧的首尾两条。纠偏轮按更新后的计划 §4 重做：

1. **变化键改为内容/状态指纹**，由 db 观察加载器（`load_activity_ledger_observations`）计算并放到 `ActivityLedgerObservation.content_fingerprint` 上，engine 不再查库：
   - `frame`：`frames.content_hash`；为 NULL 时回退到 SQL 端计算的轻量文本指纹（`length + head/tail hex`，避免把大文本拉进元数据查询；真实库 32958 帧中仅 533 帧 hash 为 NULL，该回退很少触发）；
   - `ui_event`：`(event_type, element_name, element_value, text_content)` 组合串；
   - `audio`：无指纹（不参与变化比较，按规则全留）。
2. **连续去重、分来源链**：frame 与 ui_event 各自与同类上一条候选比较；被丢弃的重复候选与保留者指纹相同，故与"上一条观察比较"等价于与"上一条被保留的候选比较"，`A→B→A` 保留 3 行（有测试）。
3. **首尾恒保留**（边界锚点，即使与相邻指纹相同）——有测试。
4. **`empty_text` 恒不可达问题**：改为 `empty_content`，由 loader 如实标记——audio 查询去掉了 `length(trim(transcription)) >= 8` 预过滤（空转写流入并标记，由策略丢弃+记账，符合更新后 §4.2 "不允许该规则实际不可达"）；帧的 `content_hash` 与两种文本皆空时同样标记。空转写不再产生动作（`action_for` 对 `content_empty` 短路）；全候选皆空的段不建 interval（无可锚定时间线）。
5. 上限（800/120/240）、采样、会议豁免、原因优先级、记账、幂等：**未动**。

## 1. 修改的文件（全部在允许路径内；lib.rs / db/mod.rs / server.rs 未动）

| 文件 | 改动 |
| --- | --- |
| `crates/screenpipe-db/src/migrations/20260911130000_activity_retention_and_summaries.sql` | **新增迁移**：`activity_interval_summaries` + `activity_interval_retention` 两表（CHECK、CASCADE、时间戳默认值、`idx_activity_summaries_updated`），单事务，标准 header |
| `crates/screenpipe-db/src/db/activity_ledger.rs` | ① `ActivityLedgerObservation`/`RawObservation` 加 `content_fingerprint` + `content_empty`；② 观察加载器三个 SQL：帧指纹（hash→文本回退→NULL）、ui_event 组合指纹、audio 去 `>=8` 过滤并标记空转写；③ `ActivityIntervalDraft.retention`、reconcile 同事务记账（kept 取库内计数、先删后写幂等）；④ 三个只读查询 + `activity_meeting_spans`；⑤ 8 个 db 测试（含指纹回退/空标记测试） |
| `crates/screenpipe-engine/src/activity_ledger.rs` | `RetentionLimits{800/120/240}`（可注入）；`RetentionCandidate{content_fingerprint, empty_content}`；`select_retained_evidence`（分来源连续指纹去重 + 首尾 + empty/duplicate + 采样）；`drop_reason_priority`；`is_meeting_interval`；`build_ledger_with_policy`；`action_for` 跳过空内容观察；finish_segment 跳过零证据段；12 个策略测试 |
| `crates/screenpipe-engine/src/routes/activity_ledger.rs` | 三个只读 handler + `ActivityIntervalDetail`/`ActivityLedgerRetentionEntry` + `clamp_history_range` 复用；映射测试 |
| `crates/screenpipe-db/tests/legacy_db_upgrade.rs` | 升级断言补两条 B01 表存在性检查 |

一次性分析脚本（不入库）：`.agent/tmp/retention-compare.py`。

## 2. 计划 §4（更新后）规则 → 实现位置

| 规则 | 位置 |
| --- | --- |
| 1 内容指纹变化去重（同类、连续、`A→B→A` 3 行）+ 首尾恒保留 | db loader SQL 指纹列；`select_retained_evidence` 的 `last_fingerprints` 链（per-source-type）与边界覆盖 |
| 2 转写全留、空文本如实标记（不再不可达） | audio SQL 去长度过滤 + `content_empty` 标记；策略 `empty_content → "empty"`；`action_for` 空内容不出动作 |
| 3 会议豁免 | `activity_meeting_spans`（NULL end 视为延伸）+ `is_meeting_interval`（重叠之和×2 > 时长） |
| 4 总量 800 + 采样优先变化点/首尾 | `RetentionLimits.total_cap` + `sample_out`（优先集自身超限时二次收敛并保首尾），原因 `sample` |
| 5 单来源 120/240 | `frame_cap/ui_event_cap`，同一 `sample_out`，原因 `cap` |
| 6 记账 + 原因优先级 | engine 聚合 drops → `drop_reason_priority` 取主导；db 同事务写 `activity_interval_retention` |
| 7 幂等 | 确定性排序；证据 `ON CONFLICT` 不删已保留；记账 upsert；`reconcile_rerun_keeps_retention_and_evidence_stable` |

## 3. 验收命令与结果（纠偏后重跑）

| # | 命令 | 结果 | exit |
| --- | --- | --- | --- |
| 1 | `cargo test -p screenpipe-db --lib activity` | **13 passed; 0 failed** | 0 |
| 2 | `cargo test -p screenpipe-engine --lib activity_ledger` | **22 passed; 0 failed** | 0 |
| 3 | `cargo test -p screenpipe-engine --test knowledge_correctness` | **13 passed; 0 failed** | 0 |
| 4 | `SCREENPIPE_LEGACY_DB=/tmp/knowledge-rename-backup-2026-09-10/zhiji-dev-db.sqlite cargo test -p screenpipe-db --test legacy_db_upgrade` | **1 passed**（新表创建、fk 空、integrity ok） | 0 |
| 5 | `cargo check -p screenpipe-db -p screenpipe-engine -p screenpipe-connect` | 通过，我方文件无告警 | 0 |

新增的引擎测试直接覆盖纠偏要求的断言：`N 个不同指纹 → N 行`（`aba_content_sequence_keeps_all_three_rows`、`visual_evidence_keeps_only_content_change_points_and_boundaries`）、`连续相同 → 首+尾`（`consecutive_identical_content_keeps_first_and_boundary_only`）、`A→B→A → 3 行`、`NULL content_hash 回退 full_text/accessibility_text`（db 侧 `frame_fingerprint_prefers_content_hash_and_falls_back_to_text`）、`变化点在上限采样中优先保留`（`per_source_cap_samples_and_reports_cap_over_unchanged`）、`首尾恒保留（与相邻相同也保留）`、全空帧 `empty` 丢弃与全空段跳过。

额外回归：`cargo test -p screenpipe-db --lib` 全量 **171 passed; 0 failed**（2 ignored，既有）。

## 4. 实测对比（`~/.screenpipe-dev/db.sqlite` 只读副本 → `/tmp/b01-compare.sqlite`）

方法：`.agent/tmp/retention-compare.py` 逐 interval 重放观察加载器（frames 10s 采样、ui_events 5 类事件、audio），分别按旧策略（首条+尾条+动作证据）与新策略（§4）计数，含上限采样模拟。3779 个间隔（2026-09-02 ~ 09-09）。

```
intervals=3779
OLD  kept rows: sum=9032  mean=2.39  median=2  max=60
NEW  kept rows: sum=9097  mean=2.41  median=2  max=50
cap-triggered intervals: 0 (per-source), 0 (total)
intervals where NEW keeps MORE than OLD: 647; equal: 2841
by candidate-count bucket (intervals, old sum, new sum):
   0-2: n=2505  old=3111  new=3097
   3-9: n=1057  old=3363  new=3797
  10-49: n=209  old=2235  new=1946
   50+: n=8    old=323   new=257
largest gains (id, candidates, old, new):
  (11730, 50, 2, 50) (13420, 26, 2, 25) (11826, 20, 2, 20)
  (11625, 19, 2, 19) (12312, 19, 2, 19) (13375, 20, 3, 20)
interval 12210（纠偏指令探查的段，4m17s、106 原始帧）: candidates=35, old=14, new=23
```

解读：
- **长内容段的收益兑现**：11730 从 2 行 → 50 行（全部为内容变化点+首尾），12210 从 14 → 23；3-9 候选桶整体 old 3363 → new 3797。
- **总量被超短段主导**：2505/3779 个段候选 ≤ 2（秒级小段），所以总量均值几乎持平；这不是规则退化，是段分布的形状。
- **10-49/50+ 桶 new < old 的原因**：旧策略无条件保留全部动作证据（含连点同一按钮、短转写外的一切 ui_event）；新策略按指纹把"连续相同的事件/画面"视为未变化——例如连续 10 次点击同一按钮只保留首个（及可能的尾条）。这是更新后计划 §4 的字面行为。
- cap（120/240/800）在当前库上零触发：观察加载器按 10s×(app,window,url,doc) 组合采样后，最大段仅 50 候选。上限仍是必要护栏（帧密度提高或口径调整时生效）。
- 库中 533/32958 帧 content_hash 为 NULL，其中 498 帧连文本也空——这些帧现在会以 `empty` 原因被丢弃并记账（此前 `>=8` 过滤同样不加载它们，短转写 29/50 条则按"只丢空文本"新规则进入证据流）。

## 5. 未做 / 未验证项

1. **三个读接口未挂载**（`server.rs` 不在允许路径）：handler 已 pub、已编译、逻辑有测试；需编排方补 6 行（3 import + 3 `.get()`，路径见下）。HTTP 端到端未验证。
   - `.get("/activity-intervals", get_activity_intervals)` / `.get("/activity-intervals/missing-summary", get_activity_intervals_missing_summary)` / `.get("/activity-intervals/:interval_id/evidence", get_activity_interval_evidence)`
2. **摘要表无写入方**（B02）；本批验证 DDL、级联与查询读取。
3. **实测对比的范围**：脚本未纳入会议豁免（本库 3779 段的判定需 meetings 表 join，cap 零触发使其影响仅限豁免上限的场景）；帧的文本回退指纹用 SQL 轻量代理（长度+头尾 64 字节 hex）而非全文哈希，SQLite 端无内置哈希函数，且该回退仅覆盖 1.6% 的帧。
4. **语义裁量**（与首轮相同，未变）：采样作用于去重后保留集，优先集自身超限时二次均匀收敛并保首尾；`kept` 以同事务内库中证据行数为准。

## 6. 已知风险

- **ui_event 去重对动作密集段的影响**：连点同一控件、同一剪贴板事件等"指纹相同"的动作序列现在只保留首个（+尾条锚点）；动作行（activity_actions）仍全量存在，仅证据引用变少。B02 摘要若需要动作频次，可从 actions 表补。
- **audio 口径放宽**：1-7 字符短转写不再被 loader 预过滤（本库 50 条转写中 29 条属此类），进入证据与动作流；空转写不产生动作。
- **帧指纹回退是代理指纹**：同长度且头尾 64 字节相同的全文变化会漏判（仅 1.6% 的帧走此路径）。
- 保留集形状变化对 `extract.rs` 的影响与首轮相同：本批未动提取，B03 切换时需真实区间比对。
