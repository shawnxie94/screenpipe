# 活动层收口 B02b-1：读源收敛（时间线改读数据库摘要）执行计划

```yaml
status: approved
owner: shawn
created: 2026-09-11
framework: docs/trd/personal-workbench-integration-framework.md#d-13
covers:
  - D-13 读源收敛：活动时间线读数据库摘要，不再把 KV 叙事当唯一来源
doc-covers: apps/screenpipe-app-tauri/lib/activity-history-persistence.ts, apps/screenpipe-app-tauri/components/activity-ledger.tsx, apps/screenpipe-app-tauri/src-tauri/src
doc-verified: 96fde4b0a
```

## 1. 目标与非目标

### 1.1 目标（B02b-1）

活动时间线（`activity-ledger.tsx`）优先显示**数据库里的间隔摘要**（B02a 生产、真实模型生成），KV 里的旧叙事降级为兜底；两条路径返回同一前端契约，UI 视觉不变。

### 1.2 非目标（B02b-2）

| 项 | 批次 |
| --- | --- |
| 下线旧生成器（`generate_activity_history` + 其 `SYSTEM_PROMPT` + Pi 调用与镜像写入） | B02b-2 |
| 清理/迁移 KV 旧叙事、移除 `activity-review-prompt.ts` 残留 | B02b-2 |
| WorkUnit 先摘要后召回、字段扩展、会话计数按活动去重 | B03 |
| 四个 skill | B04 |

本批**保留**旧生成器可用（不改其行为、不删代码），只是 UI 不再必经它；旧 KV 数据继续可读，保证回退安全。

## 2. 现状锚点（已核对）

| 事实 | 位置 |
| --- | --- |
| 前端持久化：设置库（加密）键 `activityHistory:<producer>`，含 `entries` 与 `coverage` | `apps/screenpipe-app-tauri/lib/activity-history-persistence.ts`（`loadPersistedActivityHistory` / `reconcilePersistedActivityHistory`） |
| 时间线组件：读持久化文档 + 触发 `generate_activity_history`、`get_activity_history` | `apps/screenpipe-app-tauri/components/activity-ledger.tsx`、`lib/utils/tauri.ts:449,470` |
| 旧生成器（第二套总结） | `apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs`（`SYSTEM_PROMPT`、`generate_activity_history`） |
| DB 侧已有读接口 | `GET /activity-intervals`、`/activity-intervals/:id/evidence`（B01 挂载）、`activity_intervals_between` 查询 |
| Tauri 命令约定 | `.claude/skills/screenpipe-tauri/SKILL.md`：`#[tauri::command]` + `#[specta::specta]`，随后 `bun run bindings:generate` / `bindings:check` / `typecheck` |

## 3. 契约

新 Tauri 命令（放在 `src-tauri/src/activity_summaries.rs`，从 `activity_history.rs` 独立出来）：

```rust
#[tauri::command]
#[specta::specta]
pub async fn get_activity_interval_summaries(
    state: State<'_, ...>,
    start: String,   // RFC3339
    end: String,     // RFC3339
    limit: Option<u32>,
) -> Result<ActivityIntervalSummariesResponse, String>
```

返回条目**逐字段对齐现有前端 `ActivityHistoryEntry`**（避免 UI 大改）：

| 前端字段 | 数据库来源 |
| --- | --- |
| `id` | `work:<interval_id>` / `meeting:<interval_id>` |
| `kind` | 会议间隔（`activity_meeting_spans` 重叠过半）→ `meeting`，否则 `work` |
| `start_at` / `end_at` | `activity_intervals.start_at/end_at` |
| `title` | `activity_tasks.title`（+ `app_name` 作为副标题字段若前端需要） |
| `summary` | `activity_interval_summaries.summary`（**无摘要的间隔不返回**） |
| `keywords` | 新字段（前端可选用，缺失不影响既有渲染） |
| `evidence` | `evidence_refs` 映射为 `{source_type, source_id, occurred_at}`（`occurred_at` 从 `activity_evidence` 取） |
| `coverage` | 响应级：`{start, end}` 请求区间 + 是否 truncated |

前端：

1. `lib/activity-history-persistence.ts` 增 `loadActivitySummariesFromDb(range)`：调新命令；失败时抛错由上层兜底（不静默吞）。
2. `activity-ledger.tsx`：范围内的条目**优先取 DB**；DB 为空/调用失败时回落到既有 KV 路径，并在 UI 上不做额外标记（保持视觉不变），但把回落写进 `console.warn` 便于排查。
3. 不改变 `reconcilePersistedActivityHistory` 的行为（旧生成器仍可用，B02b-2 再处理）。

## 4. 测试与验收命令

| # | 命令 | 期望 |
| --- | --- | --- |
| 1 | `cd apps/screenpipe-app-tauri && bun run bindings:generate && bun run bindings:check` | `tauri.ts` 与 Rust 一致（新命令与类型已导出） |
| 2 | `cd apps/screenpipe-app-tauri && bun run typecheck` | exit 0 |
| 3 | `cd apps/screenpipe-app-tauri && bunx vitest run lib/activity-history-persistence.test.ts components/activity-ledger.test.tsx` | 通过；新增：DB 优先、DB 为空回落 KV、DB 报错回落 KV、条目字段映射 |
| 4 | `cd apps/screenpipe-app-tauri && bun run test:tauri activity_summaries` | 新增 Rust 单测：SQL 映射、无摘要不返回、区间过滤、limit 截断标记 |
| 5 | `cargo test -p screenpipe-db --lib activity && cargo test -p screenpipe-engine --test knowledge_correctness` | 17 / 13 passed（未改动数据层时的回归） |
| 6 | `cargo check -p screenpipe-db -p screenpipe-engine` | exit 0 |

浏览器 mock 循环（`apps/screenpipe-app-tauri/README.md`）可用于 UI 验证：无需 Tauri 构建即可看 DB 路径渲染。

## 5. 风险与回退

| 风险 | 处置 |
| --- | --- |
| DB 摘要在老区间为空 → 时间线突然变空 | 回落 KV；DB 空不视为错误 |
| 会议判定与旧叙事不一致 | 会议判定复用 B01 的 `activity_meeting_spans` 口径，测试固定该口径 |
| 新增命令破坏 bindings 幂等 | 用 `bindings:check` 门禁；Rust 与 `tauri.ts` 同提交 |
| 前端 24 个既有失败干扰判断 | 只跑与改动文件相关的测试文件，并记录基线对比 |

**回退**：UI 只加一条优先路径；去掉 DB 分支即可回到纯 KV。

## 6. 交付边界（子任务约束）

- 允许路径：`apps/screenpipe-app-tauri/src-tauri/src/activity_summaries.rs`（新）、`src-tauri/src/{{main,specta_bindings}.rs, activity_history.rs（仅在需要共享类型时）}`、`apps/screenpipe-app-tauri/lib/activity-history-persistence.ts`、`apps/screenpipe-app-tauri/lib/utils/tauri.ts`（生成物）、`apps/screenpipe-app-tauri/components/activity-ledger.tsx`、对应测试文件。
- 禁止：改 `crates/**`（本批不需要）、删旧生成器、改 `activity-review-prompt.ts`、改 `docs/**`、动 KV 数据结构。
- 不做：ui 视觉重设计、i18n 变更、无关重构；真实模型调用（测试用 mock/夹具）。
- 真实库只读；不得写 `~/Library/Application Support`、`~/.screenpipe`、`~/.screenpipe-dev`。

## 7. 后续

| 批次 | 内容 |
| --- | --- |
| B02b-2 | 下线旧生成器与 KV 读源，清理 `activity-review-prompt.ts` 残留 |
| B03 | WorkUnit 先摘要后召回 + 流程/环境/细节字段 + 会话计数按活动去重 |
| B04 | 四个 skill（共享取数 + 活动总结 / 工作单元 / 知识提炼） |
