# 活动层收口 B02a：间隔摘要生成（单一总结器的生产侧）执行计划

```yaml
status: approved
owner: shawn
created: 2026-09-11
framework: docs/trd/personal-workbench-integration-framework.md#d-13
covers:
  - D-13 摘要必须自足且分档（<15min 80–150 字 / 15–60min 150–300 / >60min 300–600；关键字 5–12 含专名；每条 1–3 条证据）
  - D-15 先摘要后召回（本批只生产摘要，不改 WorkUnit 取数）
doc-covers: crates/screenpipe-engine/src/knowledge, crates/screenpipe-db/src/db/knowledge, crates/screenpipe-db/src/migrations
doc-verified: 9595528ea
```

## 1. 目标与非目标

### 1.1 目标（B02a）

让 B01 建好的 `activity_interval_summaries` 真正有写入方：一个新的 `Summarize` 任务种类，由桌面 worker 在真实模型通道上为「已结束且还没有摘要的间隔」生成自足摘要 + 关键字，落库并可幂等重算。

### 1.2 非目标（明确留给 B02b / B03 / B04）

| 项 | 批次 |
| --- | --- |
| 前端读源从 KV 切到数据库、旧叙事标 legacy、活动台账 UI 改造 | B02b |
| WorkUnit 先摘要后召回、流程/环境/细节字段、会话计数按活动去重 | B03 |
| 四个 skill（共享取数 + 三个任务） | B04 |
| 旧「活动历史」生成路径的下线 | B02b（本批只保证新路径独立可用、不与之竞争写同一张表） |

## 2. 现状锚点（已核对）

| 事实 | 位置 |
| --- | --- |
| 模型调用边界：engine 不解析凭据、不选模型，桌面注入 `KnowledgeModelExecutor`；每次调用计入每 job 预算（≤3） | `crates/screenpipe-engine/src/knowledge/executor.rs` |
| worker：`JobHandlers` 注册表 + `start_public` 循环 + `definition_for_kind` + `kind_for_definition` | `crates/screenpipe-engine/src/knowledge/worker.rs:76-260` |
| 任务种类枚举 / 字符串 / 优先级 / 任务绑定 | `crates/screenpipe-db/src/db/knowledge/types.rs:181+`、`crates/screenpipe-db/src/db/knowledge/jobs.rs:11,115` |
| 提示词为编译期常量，版本串参与 input hash | `crates/screenpipe-engine/src/knowledge/prompts/mod.rs`、`sources.rs:22`（`EXTRACT_PROMPT_VERSION`） |
| 发现式入队范例（结算后的间隔 → 任务） | `crates/screenpipe-engine/src/knowledge/extract.rs:25-73`（`discover_and_enqueue`） |
| 桌面侧：5 分钟 ticker 调 discovery；handler 注册在同一处 | `apps/screenpipe-app-tauri/src-tauri/src/knowledge_runtime.rs:440-480` |
| B01 已提供：摘要表、`activity_intervals_missing_summary`、`activity_evidence_for_interval`、保留记账 | `crates/screenpipe-db/src/db/activity_ledger.rs`、迁移 `20260911130000_*` |

## 3. 数据（新迁移，不改已应用迁移）

新迁移 `crates/screenpipe-db/src/migrations/<ts>_activity_summary_evidence_refs.sql`：

```sql
ALTER TABLE activity_interval_summaries ADD COLUMN evidence_refs TEXT NOT NULL DEFAULT '[]';
CREATE INDEX idx_activity_summaries_input ON activity_interval_summaries(input_hash);
```

- `evidence_refs`：JSON 数组，元素形如 `{"source_type":"frame","source_id":123}`，1–3 条，必须来自该间隔已保留的证据行（写入前校验，越界拒绝）。
- 单事务、无 `no-transaction`；新表在 B01 迁移之后，真实库升级路径仍由 `legacy_db_upgrade` 覆盖。

## 4. 生成规则

1. **分档按间隔时长**（`end_at - start_at`）：`< 15 分钟` → `short`（80–150 字）；`15–60 分钟` → `medium`（150–300 字）；`> 60 分钟` → `long`（300–600 字）。字数为去除空白后的字符数。
2. **摘要必须自足**：覆盖正在做的事、推进方式、关键决定与异常、涉及的工具/文件/项目/协作对象、结果与未完成项。禁止「处理了 X 工作」这类空话（提示词里给出反例）。
3. **关键字 5–12 个**，含专名（项目/文件/人名/工具）；去重、去空。
4. **引证 1–3 条**：只能引用提示词中给出的证据编号（`e1`、`e2`…，映射回真实 `source_type/source_id`）；校验失败即视为非法输出。
5. **一次修复重试**：字数越界、关键字条数越界、引证越界三类可修复错误，允许用严格化提示词重试一次；仍不合格则该 job 失败（不写库），错误码 `summary_invalid`，不静默存垃圾。
6. **输入构造**：间隔身份（应用/窗口/标题/时长）+ 已保留证据的**有界**摘录（复用 `sources.rs::bounded_excerpt`，总字符上限沿用提取路径的 24k 量级）+ 保留记账（哪些被采样/丢弃，供摘要措辞不夸大覆盖）。
7. **预算**：计入既有每 job 预算（首次 + 修复 = 2 次调用，≤3 上限）。
8. **时间窗**：只处理已结算的间隔（`end_at <= now - 5 分钟`），避免给仍在进行的段生成摘要。

## 5. 接入点

| 位置 | 改动 |
| --- | --- |
| `KnowledgeJobKind` | 新增 `Summarize`：`as_str() = "summarize"`、优先级高于 `Extract`、`task_binding` = `("knowledge.summarize", "extract")`、legacy kind 串 `knowledge_summarize` |
| `worker.rs` | `definition_for_kind` 加分支；public loop 的 `definitions` 列表加入（有 handler 才认领） |
| `knowledge/summarize.rs`（新） | `discover_and_enqueue`（挑 `activity_intervals_missing_summary`，单轮上限 20，幂等由 active-input 唯一性保证）+ `summarize_handler()` + `run_summarize` |
| `prompts/mod.rs` | `SUMMARY_SYSTEM` / `SUMMARY_USER` 常量 + `SUMMARY_PROMPT_VERSION`（参与 input hash） |
| `knowledge_runtime.rs`（src-tauri） | ticker 内 discovery 先摘要后提取；`handlers.register(Summarize, summarize::summarize_handler())` |
| `db` | `activity_summary_upsert(interval_id, …)`（幂等：同 `input_hash` 直接返回已存在行）+ 读取用 B01 的查询面 |

`input_hash` = hash(`SUMMARY_PROMPT_VERSION` + 间隔身份 + 已保留证据 id 列表 + 模型身份)。**证据变化或提示词/模型变化都要重算**：`input_hash` 不同即视为待生成。

## 6. 测试与验收命令

| # | 命令 | 期望 |
| --- | --- | --- |
| 1 | `cargo test -p screenpipe-engine --lib summarize` | 新增：分档选择、越界修复重试一次、引证校验、同 input_hash 不重复调用、证据变化触发重算、预算上限 |
| 2 | `cargo test -p screenpipe-db --lib activity` | 全绿；新增：`evidence_refs` 校验、upsert 幂等、按 input_hash 查待生成 |
| 3 | `SCREENPIPE_LEGACY_DB=/tmp/knowledge-rename-backup-2026-09-10/zhiji-dev-db.sqlite cargo test -p screenpipe-db --test legacy_db_upgrade` | 通过（新列在真实库升级路径上创建） |
| 4 | `SCREENPIPE_ACTIVITY_DB=/tmp/b01-compare.sqlite cargo test -p screenpipe-engine --test activity_summary_real_db` | 新增：真实 3779 个间隔的库副本上，用假执行体跑 discovery + handler，断言写了摘要行、二次运行 0 次新调用、非空摘要字数落在对应档位 |
| 5 | `cargo test -p screenpipe-db --test knowledge_correctness && cargo test -p screenpipe-engine --test knowledge_correctness` | 8 / 13 passed（提取路径未改） |
| 6 | `cargo check -p screenpipe-db -p screenpipe-engine` | exit 0 |
| 7 | `cd apps/screenpipe-app-tauri && bun run test:tauri knowledge_runtime` | src-tauri 改动必须走仓库 native 命令；不可用则记为未执行 |

## 7. 风险与回退

| 风险 | 处置 |
| --- | --- |
| 给每个间隔都调模型 → 成本/打扰 | 只处理已结算且缺摘要的间隔；单轮上限 20；同 `input_hash` 不重复调用；沿用每 job 预算 |
| 摘要与旧「活动历史」叙事不一致 | 本批只新增生产路径，不切读源；B02b 切读源时对同一段时间比对两者 |
| 模型输出越界（空话/超长/无引证） | 严格化重试一次，仍不合格即失败不落库；测试用假执行体覆盖三类越界 |
| 真实库 3779 个间隔一次性触发大量调用 | discovery 单轮上限 20 + 时间窗；真实验证用假执行体（不真调模型） |

**回退**：移除 handler 注册即停止生成；已写摘要行可整体删除（无读路径依赖）。

## 8. 交付边界（子任务约束）

- 允许路径：`crates/screenpipe-db/src/{db/knowledge/,db/activity_ledger.rs,db/mod.rs,lib.rs,migrations/,tests/}`、`crates/screenpipe-engine/src/knowledge/`、`crates/screenpipe-engine/src/routes/activity_ledger.rs`、`crates/screenpipe-engine/tests/`、`apps/screenpipe-app-tauri/src-tauri/src/knowledge_runtime.rs`。
- 禁止：`docs/**`、已存在的迁移文件、前端 `apps/screenpipe-app-tauri/{lib,components}/**`、旧活动历史路径（`activity_history.rs`、`activity-review-prompt.ts`）、`crates/screenpipe-engine/src/knowledge/extract.rs` 的取数逻辑（B03 才动）。
- 不做：模型/凭据解析（必须走注入的 executor）、UI 读源切换、WorkUnit 字段扩展、顺手修无关 bug。
- 真实库只读；模型调用只允许假执行体（测试），不得真调远程模型。

## 9. 后续批次

| 批次 | 内容 | 依赖 |
| --- | --- | --- |
| B02b | 读源收敛：前端活动历史改读数据库摘要、旧 KV 叙事标 legacy、UI 与 Tauri 命令适配、两套总结逻辑下线 | B02a |
| B03 | WorkUnit 先摘要后召回 + 流程/环境/细节字段 + 会话计数按活动去重 | B02a |
| B04 | 四个 skill（共享取数 + 活动总结 / 工作单元 / 知识提炼，按 `AgentLayout` 注入） | B02b、B03 |

## 完成记录（B02a）

- 子任务：`zct_880cfcfc20d94b85`（单轮；判 scope_violation，越界三文件为计划漏列的必要改动，已修正允许路径）
- 交付：迁移 `20260911140000_activity_summary_evidence_refs.sql` 与 `20260911150000_knowledge_jobs_allow_summarize.sql`；`knowledge/summarize.rs`（发现式入队 + 处理器 + 校验 + 落库 + 22 项测试）；提示词 `SUMMARY_*`；`KnowledgeJobKind::Summarize` 全链路；src-tauri 接线
- 协调会话补：`task_activate_owner_generation("knowledge")` 加入 `knowledge.summarize`；`legacy_db_upgrade` 增补队列重建保行断言
- 证据：`docs/reviews/evidence/activity-summary-b02a-2026-09-11/`
- 验收：协调会话实跑 11 条命令全绿，含真实库副本集成测试（20/20 摘要成功）与 dev 库副本队列重建保行（715 行）
- 带入 B02b：摘要质量人工评审、前端读源切换、旧叙事标 legacy、读接口 HTTP 覆盖
