# B02b-1（时间线读源收敛到数据库摘要）— 协调会话独立复验

- 子任务：`zct_719c35e0d33647a6`（单轮交付，status completed）
- 计划：`docs/plans/activity-read-source-convergence-execution-plan.md`
- 复验时间：2026-09-11（仓库已迁至 `products/screenpipe`，本次起用新路径）

## 结论：接受

## 复验命令与结果（协调会话实跑）

| # | 命令 | 结果 |
| --- | --- | --- |
| 1 | `cd apps/screenpipe-app-tauri && bun run bindings:check` | `specta_bindings::tests::tauri_bindings_are_current ... ok`（首次我用 tail 截断只看到尾随 `shutdown_tasks` 的 "1 filtered out"，误判为假绿；完整输出确认通过） |
| 2 | `cd apps/screenpipe-app-tauri && bun run typecheck` | exit 0 |
| 3 | `cd apps/screenpipe-app-tauri && bunx vitest run lib/activity-history-persistence.test.ts components/activity-ledger.test.tsx` | 60 passed / **2 failed**；失败两项与基线逐项一致（见下） |
| 4 | **基线复现**：`git stash push` 掉 `components/activity-ledger.{tsx,test.tsx}` 后重跑同一测试文件 | **2 failed / 49 passed —— 同样两项失败**，证明非本批引入（断言期望英文 prompt，而 `activity-review-prompt.ts` 本就是中文且零 diff）；随后 `git stash pop` 已还原 |
| 5 | `cd apps/screenpipe-app-tauri && bun run test:tauri activity_summaries` | 9 passed（字段映射 / 无摘要不返回 / 区间边界 / limit 截断 / 会议过半口径） |
| 6 | `cargo test -p screenpipe-db --lib activity` | 17 passed |
| 7 | `cargo test -p screenpipe-engine --test knowledge_correctness` | 13 passed |
| 8 | `cargo check -p screenpipe-db -p screenpipe-engine` | 0 errors |

## 代码复核（读 diff）

- **契约对齐**：`ActivityIntervalSummaryEntry` 的字段与前端既有 `ActivityHistoryEntry` 一一对应（id=`<kind>:<interval_id>`、kind、start_at/end_at、title、summary、evidence），`evidence.kind` 映射为 `"screen"|"audio"`，与 `ActivityHistoryEvidence` 的类型闭集一致；keywords 作为附加字段透传、不回写 KV。
- **取值语义**：区间按 `start_at ∈ [start, end)` 归属（避免跨页重复）；**无摘要的间隔不返回**；会议判定复用 B01 的 `activity_meeting_spans` 且要求重叠 ≥ 一半；每条最多 3 条引证（与旧生成器预算一致）；limit 默认 500、超出置 `truncated`。
- **回落行为**：`loadActivitySummariesFromDb` 失败时抛错（不静默），组件在「DB 为空」与「DB 报错」两条路径都 `console.warn` 后回落 KV；初始加载与 `activity-history-updated` 刷新共用同一 loader；coverage 仍取 KV（旧生成器补洞调度不变，留待 B02b-2）。
- **命令登记**：`lib/utils/tauri.ts` 由 `bindings:generate` 生成，`main.rs` 只加 `mod activity_summaries;`（未手改命令清单），符合 skill 约定。

## 未验证 / 带入 B02b-2

1. **真实 UI 数据流未跑**：本批无真机验证（DB 摘要路径在真实数据下的渲染留待桌面复核）；建议在 B02b-2 随读源切换一起用 dev 实例验证。
2. **旧生成器仍在**：`generate_activity_history`（Pi 提示词）继续运行并写 KV，属 B02b-2 下线范围；本批只是 UI 优先读库。
3. **两个既有失败仍在**：`makes recorded meetings mandatory interpretation anchors`、`can draft a skill from every activity interval`（英文断言 vs 中文化实现），与基线一致，未修（不在本批范围）。
