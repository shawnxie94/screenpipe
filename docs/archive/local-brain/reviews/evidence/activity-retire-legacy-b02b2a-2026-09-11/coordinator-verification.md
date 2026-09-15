# B02b-2a（停用旧叙事自动生成）— 协调会话独立复验

- 子任务：`zct_08e95e36341747f0`（completed，无越界）
- 计划：`docs/plans/activity-retire-legacy-narrative-generation-plan.md`
- 复验时间：2026-09-11

## 结论：接受

## 复验命令与结果（协调会话实跑）

| # | 命令 | 结果 |
| --- | --- | --- |
| 1 | `cd apps/screenpipe-app-tauri && bun run typecheck` | exit 0 |
| 2 | `cd apps/screenpipe-app-tauri && bunx vitest run components/activity-ledger.test.tsx lib/activity-history-persistence.test.ts` | **2 failed / 52 passed（54）**；失败项与基线**同名**：`makes recorded meetings mandatory interpretation anchors`、`can draft a skill from every activity interval` |
| 3 | `cd apps/screenpipe-app-tauri && bun run bindings:check` | `tauri_bindings_are_current ... ok`；且 `git diff lib/utils/tauri.ts` 为空（本批未动生成物） |
| 4 | `grep -rn generateActivityHistory components lib`（排除测试） | 仅 `lib/utils/tauri.ts:447` 的生成绑定（计划 §3.3 明确保留，命令保号） |

## 删除范围的核查（我重点看了"刷新按钮"是不是兼作重载）

被删的两个按钮都是**生成触发器**，不是普通重载：

- 头部按钮 `aria-label="刷新历史记录"` → 原 `regenerateSelectedRange("refresh")` → 调生成
- 空态按钮 → 原 `regenerateSelectedRange("empty_state")` → 调生成

重载能力未丢：区间变化与 `activity-history-updated` 事件都仍走 `loadHistorySnapshot`（DB 优先、KV 兜底）。空态文案由"点按钮生成"改为被动提示，符合"只保留一套生产者"。

一并删除的还有生成专用的本地状态与调度（`historyLoading`/`historyAbortRef`/`GenerationSource`/补洞调度 `recentActivityUnlockDelay` 与解锁 effect），加载路径不再有"等待生成"分支——这正是本批的目的。

## 保留项（按计划）

- Rust 生成器 `activity_history.rs`、命令 `generate_activity_history`、`lib/utils/tauri.ts` 绑定：保号不删（物理下线有门禁：需用户先在 S1 用真实模型验证新摘要质量）
- KV 读源 `get_activity_history` 与 `activity-history-updated` 监听：保留（老区间只有 KV 有内容）
- `lib/activity-history-persistence.ts`、`src-tauri/src/activity_history.rs` 顶部已加 legacy 注释

## 未验证 / 风险

1. **真机端到端未验证**：仅组件测试覆盖；新用户启用后到 B02a Summarize 产出前会看到空态/被动提示，属预期行为，建议随 S1 的真机验收一并观察。
2. **删除 8 个生成专属用例**（生成错误文案 ×3、慢生成、funnel 空壳、卸载在飞、离开页面、quality failure）：这些断言的载体已不存在；DB 优先/KV 兜底等其余用例保留，测试总数 62 → 54。
3. 两个既有失败（英文断言 vs 中文化）仍在，未修（不在本批范围）。
