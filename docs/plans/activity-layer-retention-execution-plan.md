# 活动层收口（B01：证据按变化保留 + 摘要落库）执行计划

```yaml
status: approved
owner: shawn
created: 2026-09-10
framework: docs/trd/personal-workbench-integration-framework.md#d-13
covers:
  - D-13 单一活动层（间隔 → 摘要 + 关键字 + 原始引用）
  - D-14 取数由执行体自查（本批只准备查询面，不写 skill）
  - D-15 WorkUnit 先摘要后召回（本批只落摘要，不动 WorkUnit）
doc-covers: crates/screenpipe-db/src/db/activity_ledger.rs, crates/screenpipe-engine/src/activity_ledger.rs, crates/screenpipe-db/src/migrations
doc-verified: 5ce76c304
```

## 1. 目标与非目标

### 1.1 目标（B01）

把活动层从“每段只留首尾两条证据”升级为**按变化保留 + 丢弃记账 + 摘要落库**，为后续批次（单一总结器、WorkUnit 先摘要后召回）提供稳定数据面。

### 1.2 非目标（明确留给后续批次）

| 项 | 批次 |
| --- | --- |
| 生成摘要的模型调用与提示词（含 80–150 / 150–300 / 300–600 分档） | B02 |
| 前端活动历史读源切到数据库、旧叙事标 legacy | B02 |
| WorkUnit 先摘要后召回、新增流程/环境/细节字段、会话计数按活动去重 | B03 |
| 四个 skill（共享取数 + 活动总结 / 工作单元 / 知识提炼） | B04（独立计划） |

本批**不产生模型调用**、不改任何提示词、不动 `knowledge_items` 提取流程的输入（`extract.rs` 本批不碰）。

## 2. 现状锚点（已核对）

| 事实 | 位置 |
| --- | --- |
| 台账表：`activity_tasks` / `activity_intervals` / `activity_actions` / `activity_evidence` / `activity_ledger_state`，源删除触发器已存在 | `crates/screenpipe-db/src/migrations/20260817000000_create_activity_ledger.sql` |
| 证据选择=首条 + 尾条 + 有 action 的证据 | `crates/screenpipe-engine/src/activity_ledger.rs:255-257` |
| 证据写入（含 `ON CONFLICT(interval_id, source_type, source_id)`） | `crates/screenpipe-db/src/db/activity_ledger.rs:545` |
| 提取当前直接读原始证据行（`MAX_SOURCES=32`、`MAX_PACK_CHARS=24_000`，按时间取前 32 条） | `crates/screenpipe-engine/src/knowledge/extract.rs:180-260` |
| 第二套总结：前端活动回顾（提示词禁止调用工具） | `apps/screenpipe-app-tauri/lib/activity-review-prompt.ts`（`activity-history-pi-v9`） |
| 前端持久化（KV）：`activityHistory:<producer>` | `apps/screenpipe-app-tauri/lib/activity-history-persistence.ts` |
| 历史条目镜像表 `knowledge_history_entries`（写入 `apps/.../src-tauri/src/activity_history.rs:2724` → `knowledge_migration.rs:222`） | `crates/screenpipe-db/src/db/knowledge/history.rs` |

## 3. 数据模型（本批新增，均为新增迁移，不改已应用迁移）

新迁移文件 `crates/screenpipe-db/src/migrations/<ts>_activity_retention_and_summaries.sql`：

```sql
-- 1) 摘要落库：与间隔一对一，独立表以便重生成与版本追踪
CREATE TABLE activity_interval_summaries (
    interval_id INTEGER PRIMARY KEY REFERENCES activity_intervals(id) ON DELETE CASCADE,
    summary TEXT NOT NULL CHECK (length(summary) > 0),
    keywords TEXT NOT NULL DEFAULT '[]',      -- JSON 数组，字符串，去重后 <= 8 个
    band TEXT NOT NULL CHECK (band IN ('short','medium','long')),
    summary_chars INTEGER NOT NULL,
    producer TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    model TEXT,
    input_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (...),
    updated_at TEXT NOT NULL DEFAULT (...)
);
CREATE INDEX idx_activity_summaries_updated ON activity_interval_summaries(updated_at, interval_id);

-- 2) 丢弃记账：每个间隔每个来源一条，只记计数与原因，不复制证据
CREATE TABLE activity_interval_retention (
    interval_id INTEGER NOT NULL REFERENCES activity_intervals(id) ON DELETE CASCADE,
    source_type TEXT NOT NULL CHECK (source_type IN ('frame','ui_event','audio')),
    kept INTEGER NOT NULL CHECK (kept >= 0),
    dropped INTEGER NOT NULL CHECK (dropped >= 0),
    drop_reason TEXT CHECK (drop_reason IN ('unchanged','cap','sample','empty','duplicate')),
    prompt_version TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (...),
    PRIMARY KEY (interval_id, source_type)
);
```

约束：
- 两张表都跟随间隔删除（`ON DELETE CASCADE`），不新增隐私边界——源删除依旧由既有触发器清理证据与间隔，级联带走摘要与记账。
- 迁移必须**单事务、无 `no-transaction`**，`PRAGMA foreign_key_check` 与 `integrity_check` 全程通过。
- 新表对旧库为空即可（老间隔在下次重建时补齐），**不做历史回填**——回填需要模型调用，属 B02。

## 4. 保留规则（按变化保留）

在 `crates/screenpipe-engine/src/activity_ledger.rs` 的证据选择处替换实现：

1. **按变化去重**：同一间隔内按 `occurred_at` 排序，**同类证据与上一条被保留的候选比较内容指纹**，只在变化点保留；另**始终保留区间首条与末条**（边界锚点，即使与相邻相同）。连续相同观察丢弃，原因记 `unchanged`。
   - 内容指纹：`frame` 用 `frames.content_hash`（为空时退回 `full_text` / `accessibility_text` 的哈希）；`ui_event` 用 `(event_type, element_name, element_value, text_content)` 组合。
   - **不能用 `(app_name, window_title)` 当变化键**：间隔本身已按任务身份切分，而任务身份就来自 app/window，因此该键在段内几乎不变（实测：某 106 帧间隔里该键只有 2 种取值，而 `content_hash` 有 75 种）——用它会让本规则退化成旧的首尾两条。
   - 连续去重而非全局去重：`A→B→A` 应得到 3 行（两次变化都保留）。
2. **转写不限量**：`audio` 证据全部保留（仅丢弃空文本，原因 `empty`；空文本须由 loader 如实标记，不允许该规则实际不可达）。
3. **会议全留**：与 `meetings` 表时间区间重叠超过间隔时长一半的间隔视为会议间隔，豁免单来源上限与总量上限（只丢弃空文本），全部证据保留。台帐里没有会议信号，判定用时间重叠（`meetings.meeting_start`/`meeting_end`，见 `20260225000000_create_meetings.sql`）。
4. **总量上限 800**：单个间隔保留证据总数上限 800（会议间隔除外）。超限时按**均匀采样**——按时间排序后以 `step = ceil(n / 800)` 取点，并优先保留变化点与首尾；丢弃原因记 `sample`。
5. **单来源上限**：屏幕帧 120、界面事件 240（转写与会议不限）。超限走同一均匀采样，丢弃原因记 `cap`。
6. **记账**：每间隔每来源写一行 `activity_interval_retention`（`kept` / `dropped` / `drop_reason`，多原因时取主导原因，优先级 `empty < duplicate < unchanged < sample < cap`）。
7. **幂等**：重复 reconcile 同一区间必须得到相同保留集合；已保留证据不因重跑被删（现有 `ON CONFLICT` 语义保持）。

常量集中在 engine 侧一处 `const`，便于后续调参与测试注入。

## 5. 查询面（本批提供，供 B02/B03 与 skill 使用）

在 `crates/screenpipe-db/src/db/activity_ledger.rs` 增加只读查询：

- `activity_intervals_between(start, end)`：间隔 + 任务身份 + 摘要（若有）+ 关键字 + 各来源保留/丢弃计数，供活动层读取；
- `activity_evidence_for_interval(interval_id, limit)`：按需召回原始引用（B03 的“先摘要后召回”用）；
- `activity_intervals_missing_summary(start, end, limit)`：待生成摘要的间隔（B02 用）。

路由层（`crates/screenpipe-engine/src/routes/activity_ledger.rs`）只加**读**接口，不做写入或触发模型。Tauri 侧本批不加命令（B02 切换读源时再补，避免本批同时改前端）。

## 6. 测试与验收命令

| # | 命令 | 期望 |
| --- | --- | --- |
| 1 | `cargo test -p screenpipe-db --lib activity` | 全绿；新增：保留记账写入、级联删除、幂等重跑 |
| 2 | `cargo test -p screenpipe-engine --lib activity_ledger` | 全绿；新增：内容变化去重（N 个不同指纹 → N 行）、连续相同只留首条、`A→B→A` 保留 3 行、首尾保留、转写全留、上限采样与原因优先级 |
| 3 | `cargo test -p screenpipe-engine --test knowledge_correctness` | 13 passed（输入仍为原始证据行，不受影响） |
| 4 | `cargo test -p screenpipe-db --test legacy_db_upgrade` | 通过：新表在真实库副本升级路径上创建、`foreign_key_check` 空、`integrity_check` ok |
| 5 | `cargo check -p screenpipe-db -p screenpipe-engine -p screenpipe-connect` | exit 0 |

前端与 src-tauri 本批无改动（若确实改到需按 AGENTS.md 用 `bun run test:tauri`）。

## 7. 风险与回退

| 风险 | 处置 |
| --- | --- |
| 保留策略改变了提取可见的证据集合，可能影响既有提取质量 | 本批不动 `extract.rs`；B03 才切到“先摘要后召回”，届时用同一批真实区间比对 |
| 上限/采样阈值选得不合适 | 常量集中且可测试注入；先按框架 D-13 的 120/240/800，观察后再调 |
| 大区间（如长时间会议）采样后证据过少 | 会议间隔豁免上限；转写不限量 |
| 迁移在真实库上耗时 | 只有建表与建索引，无数据搬移；真实库副本回归已覆盖 |

**回退**：新表可整体 drop（无对外读路径）；保留策略改动可用回退提交恢复旧的首尾两条实现。

## 8. 交付边界（子任务约束）

- 允许路径：`crates/screenpipe-db/src/{db/activity_ledger.rs（含观察加载器，变化指纹字段由它取出）,migrations/}`、`crates/screenpipe-engine/src/{activity_ledger.rs,routes/activity_ledger.rs}`、对应 tests。
- 禁止：`docs/**`、已存在的迁移文件、`crates/screenpipe-engine/src/knowledge/**`、前端与 `src-tauri/**`（本批无改动）。
- 不做：模型调用、提示词、KV/DB 读源切换、WorkUnit 改造、顺手修与保留策略无关的 bug。

## 9. 后续批次（本计划外，仅排序）

| 批次 | 内容 | 依赖 |
| --- | --- | --- |
| B02 | 单一总结器：间隔摘要生成（分档字数、关键字、自足口径）+ 前端读源切到数据库 + 旧叙事标 legacy | B01 的表与查询面 |
| B03 | WorkUnit 先摘要后召回 + 流程/环境/细节字段 + 会话计数按活动去重 + 提取只从 WorkUnit 取数 | B02 的摘要 |
| B04 | 四个 skill（共享取数 + 活动总结 / 工作单元 / 知识提炼，按 `AgentLayout` 注入） | B02、B03 |
