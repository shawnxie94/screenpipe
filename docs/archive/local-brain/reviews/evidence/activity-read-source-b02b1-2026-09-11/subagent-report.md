# B02b-1 交付报告：活动时间线读源收敛到数据库摘要

日期：2026-09-11　分支：`zh-local`　计划：`docs/plans/activity-read-source-convergence-execution-plan.md`（status: approved）

## 1. 结论

B02b-1 已按计划实现并全部验收通过：新增 Tauri 命令 `get_activity_interval_summaries` 从共享活动数据库（B01/B02a 产出）返回与前端既有 `ActivityHistoryEntry` 契约对齐的条目；时间线加载与 `activity-history-updated` 事件刷新改为优先读数据库摘要，DB 为空或调用失败时回落既有 KV 路径并 `console.warn` 记录原因。视觉、i18n、旧生成器行为、KV 数据结构均未改动。

## 2. 实际读取/修改的文件

**修改（6 个，全部在允许路径内）：**

| 文件 | 改动 |
| --- | --- |
| `apps/screenpipe-app-tauri/src-tauri/src/activity_summaries.rs` | **新增**。命令 `get_activity_interval_summaries(start, end, limit)`（`#[tauri::command]` + `#[specta::specta]`），响应类型 `ActivityIntervalSummariesResponse / ...Entry / ...Evidence / ...Coverage`，9 个 Rust 单测 |
| `apps/screenpipe-app-tauri/src-tauri/src/main.rs` | 仅加一行 `mod activity_summaries;`；命令清单未手改（build.rs 自动扫描） |
| `apps/screenpipe-app-tauri/lib/utils/tauri.ts` | `bindings:generate` 生成物：新命令绑定 + 4 个类型 |
| `apps/screenpipe-app-tauri/lib/activity-history-persistence.ts` | 新增 `loadActivitySummariesFromDb(range)`（失败抛错不吞）与 `mapActivitySummaryEntry` 映射 |
| `apps/screenpipe-app-tauri/components/activity-ledger.tsx` | 加 `loadHistorySnapshot`：DB 优先 → KV 兜底，两条回落路径均 `console.warn`；初始加载与 `activity-history-updated` 两个调用点复用 |
| `apps/screenpipe-app-tauri/lib/activity-history-persistence.test.ts`、`components/activity-ledger.test.tsx` | 新增 DB 优先/空回落/错回落/字段映射用例；既有用例零删除（diff 纯新增） |

**读取参考（未改）：** `activity_history.rs`、`activity-review-prompt.ts`、`crates/screenpipe-db/src/db/activity_ledger.rs`、`crates/screenpipe-engine/src/knowledge/{mod,summarize}.rs`、DB migrations、`docs/plans/…execution-plan.md`。

## 3. 契约映射（Rust 响应 → 前端 ActivityHistoryEntry）

| 响应字段 | 来源 | 前端映射 |
| --- | --- | --- |
| `id` | `"work:<interval_id>"` / `"meeting:<interval_id>"` | 原样 |
| `kind` | `activity_meeting_spans` 重叠 ≥ 1/2 → `meeting`，否则 `work`（复用 B01 口径） | 原样 |
| `start_at`/`end_at` | `activity_intervals.start_at/end_at`（DB 字符串原样透传） | 原样 |
| `title` | `activity_tasks.title` | 原样 |
| `summary` | `activity_interval_summaries.summary`；无摘要的间隔不返回 | 原样 |
| `keywords` | summaries.keywords JSON | 随条目保留（运行时附加字段，不入 KV、现有 UI 不渲染） |
| `evidence[]` | `activity_evidence_for_interval` 前 3 条 → `{source_type, source_id, occurred_at}` | `kind`：audio→audio，其余→screen；`frame_id`：source_type=frame 时取 source_id，否则 null；`at`=occurred_at；`meeting_id`/`app_name`=null；`label`="屏幕记录"/"音频记录" |
| `coverage` | 响应级 `{start, end, truncated}`；limit 默认 500，超出置 `truncated` | DB 命中时不进 KV 记账（coverage 始终取自 KV，生成调度不变） |

**区间过滤**：`start_at >= start AND start_at < end`（含起点、不含终点）。`activity_intervals_between` 是重叠语义，故在内存补此过滤（`interval_starts_in_range`）。

**与计划 §3 契约表的措辞差异（已在实现中等效处理）**：计划写 "evidence = evidence_refs 映射"。`evidence_refs` 在 DB 层无公开读接口（仅在 `activity_summary_by_input_hash` 返回，其 `input_hash` 含运行时 model identity 无法在读者侧重建；且 `crates/**` 与 `Cargo.toml` 均在禁止/允许路径外，src-tauri 默认 feature 无 sqlx 不能写裸 SQL）。按任务实现计划第 3 点指定的查询面（`activity_intervals_between` + `activity_meeting_spans` + `activity_evidence_for_interval`），evidence 取该间隔保留证据按时间序的前 3 条，形状与计划要求的 `{source_type, source_id, occurred_at}` 完全一致（occurred_at 确从 `activity_evidence` 取）。语义差别：是"保留证据前 3 条"而非"模型引用的 1–3 条"。如需严格 refs 口径，建议 B02b-2/B03 在 screenpipe-db 补一个批量读 `evidence_refs` 的接口后替换（`interval_summary_evidence` 单点改动）。

## 4. 验收命令与结果（原始输出摘录）

| # | 命令 | 结果 |
| --- | --- | --- |
| 1 | `bun run bindings:generate` | exit 0（1m43s）；`tauri.ts` 新增 `getActivityIntervalSummaries` 与 4 类型 |
| 1 | `bun run bindings:check` | exit 0，`test result: ok. 1 passed`（首尾各跑一次均过） |
| 2 | `bun run typecheck` | exit 0（首次 exit 2：helper 把 tauri 版 `kind: string` 赋给本地窄化类型，放宽返回形状后通过） |
| 3 | `bunx vitest run lib/activity-history-persistence.test.ts components/activity-ledger.test.tsx` | **60 passed / 2 failed**；persistence 文件 8/8 全过（含 DB 映射、meeting kind、错误上抛 3 个新用例）；ledger 文件 3 个新用例（DB 优先、空回落、错回落）全过。2 个失败为**既有基线失败**：`makes recorded meetings mandatory interpretation anchors` 与 `can draft a skill from every activity interval`，断言期望英文 prompt 而源文件 `activity-review-prompt.ts` 本就是中文；该文件零 diff、测试文件 diff 纯新增（无删除行），两失败输入完全由 HEAD 文件决定 → 基线即失败，与本次改动无关（符合计划 §5"既有失败干扰判断，记录基线"的预案） |
| 4 | `bun run test:tauri activity_summaries` | exit 0，`test result: ok. 9 passed; 0 failed`（覆盖：字段/keywords/evidence 映射、无摘要不返回、区间边界含端点、limit 截断标记、会议过半口径含恰 50% 边界与多 span 累加、空库响应） |
| 5 | `cargo test -p screenpipe-db --lib activity` | exit 0，**17 passed** |
| 5 | `cargo test -p screenpipe-engine --test knowledge_correctness` | exit 0，**13 passed** |
| 6 | `cargo check -p screenpipe-db -p screenpipe-engine` | exit 0 |

## 5. 环境事件（非代码改动）

首两次 `bindings:generate` 失败（exit 101），根因是工作树曾位于 `/Users/shawn/Developer/GitHub/screenpipe`（现已迁移至 `products/screenpipe`），`src-tauri/target/debug-dev` 中 `tauri` 的 build-script 缓存输出残留旧绝对路径，cargo 指纹命中重放后 `tauri_build` 读不到文件。处置：仅删除 `build/tauri-a33b1cfc1fae523d/` 与对应 `.fingerprint/` 两个 stale 目录强制该单元重跑（非 `cargo clean`，其余 375 个缓存单元保留），随后构建恢复并全部通过。

## 6. 未做 / 未验证项

- 未做（计划边界内属 B02b-2/B03）：下线旧生成器与 KV 读源、清理 `activity-review-prompt.ts`、WorkUnit 字段扩展；`generate_activity_history`/`get_activity_history` 行为零改动。
- 未验证：真实数据库上的端到端渲染（浏览器 mock 循环未跑——逻辑已被组件级单测覆盖；未构建 Tauri app 做 UI 走查，符合"不为本机 UI 验证构建原生"的约定）；`truncated` 在真实 500+ 间隔场景的表现（有单测覆盖逻辑）。
- 会议条目当前 `meeting_id=null` 且无 `kind="meeting"` 证据（`activity_evidence` 不含 meeting 源、`activity_meeting_spans` 不返回 id）：渲染走 screen/audio artifact 路径，不影响打开链接；与计划契约字段清单一致。

## 7. 风险

1. **DB 摘要稀疏期的观感**：老区间无摘要时整体回落 KV（行为与改动前一致）；混合场景下显示 DB 条目、生成调度仍按 KV coverage 走，可能对已有 DB 摘要的范围重复生成——生成结果写入 KV，不会破坏 DB，仅多耗一次模型调用（B02b-2 收口后消失）。
2. **evidence 口径差异**（见 §3）：如上游要求严格 refs 口径，需要一次数据层小改动，当前实现已在报告中显式声明。
3. **性能**：每条摘要 1 次 `activity_evidence_for_interval` 小查询（本地 SQLite 索引查询）；单日 ~百条量级无压力，7 天 × limit 500 场景未做基准。
4. 回退：UI 只加了一条优先路径，去掉 `loadHistorySnapshot` 中的 DB 分支（或让 `loadActivitySummariesFromDb` 抛错路径短路）即回到纯 KV。
