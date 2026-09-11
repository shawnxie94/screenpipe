# 活动层收口 B02b-2a：停用旧叙事自动生成（单一总结器生效）执行计划

```yaml
status: approved
owner: shawn
created: 2026-09-11
framework: docs/trd/personal-workbench-integration-framework.md#d-13
covers:
  - D-13 两套总结合一：停止旧叙事（Pi 生成）的自动触发，数据库摘要成为唯一在跑的生产者
doc-covers: apps/screenpipe-app-tauri/components/activity-ledger.tsx
doc-verified: ad5c54155
```

## 1. 目标与非目标

### 1.1 目标（B02b-2a）

让「活动时间线」不再自动调用旧叙事生成器（`generate_activity_history`，Pi + 自带提示词），从而只保留一套在跑的生产者（B02a 的 `Summarize`）。旧代码**保号不删**，可人工/回滚使用；KV 叙事继续可读。

### 1.2 非目标（延后，有门禁）

| 项 | 门禁 |
| --- | --- |
| 删除 `activity_history.rs` 的生成器与其提示词/测试（物理下线） | 用户在 S1 用真实模型验证过新摘要质量之后（`docs/reviews/evidence/` 记录） |
| 删除 KV 读源（`get_activity_history` → 历史叙事） | 同上（历史区间只有 KV 有内容） |
| 清理 `activity-review-prompt.ts` 中仅测试使用的提示词构造器 | 与物理下线同批 |
| WorkUnit 先摘要后召回、四个 skill | B03 / B04 |

## 2. 现状锚点

| 事实 | 位置 |
| --- | --- |
| 唯一的自动触发点 | `apps/screenpipe-app-tauri/components/activity-ledger.tsx:1730` 附近（`commands.generateActivityHistory`，以及围绕它的补洞调度与"生成中"状态） |
| 读源优先级（B02b-1） | 同文件 `loadHistorySnapshot`：DB 摘要优先，KV 兜底 |
| 旧生成器（保留） | `apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs` |
| 类型与纯函数（保留） | `apps/screenpipe-app-tauri/lib/activity-review-prompt.ts`（`historyDocumentFromNative`、sanitize、类型） |

## 3. 改动

1. **删除自动触发**：移除 `activity-ledger.tsx` 中调用 `commands.generateActivityHistory` 的 effect 及其专用的本地状态（生成中标记、幂等键、补洞调度触发）。保留 `activity-history-updated` 监听（成本为零，未来外部生成器仍可通知）。
2. **不再等待生成**：`cacheReady`/加载态不得因缺少生成而卡住；DB 摘要为空时按 B02b-1 的回落路径显示 KV 叙事，并在控制台给出一次明确提示（已存在）。
3. **不删 Rust 命令**：`generate_activity_history` 保留在命令面上（不调用即无害），避免动 `tauri.ts` 生成物与跨批冲突。
4. **注释标注**：在 `activity-history-persistence.ts` 与 `activity_history.rs` 顶部各加一行注释说明「KV 叙事进入 legacy，自动生成已停用（B02b-2a）；物理下线待真实模型摘要验收后执行」。
5. **测试**：更新 `components/activity-ledger.test.tsx` 中与自动生成相关的用例（改为断言**不再调用** `generateActivityHistory`；保留 DB 优先/KV 兜底用例）。**不要**顺手修两个既有失败（英文断言 vs 中文化）。

## 4. 测试与验收命令

| # | 命令 | 期望 |
| --- | --- | --- |
| 1 | `cd apps/screenpipe-app-tauri && bun run typecheck` | exit 0 |
| 2 | `cd apps/screenpipe-app-tauri && bunx vitest run components/activity-ledger.test.tsx lib/activity-history-persistence.test.ts` | 新增/改动用例通过；失败项必须**不超过基线 2 项**且仍是那两个既有失败（`makes recorded meetings mandatory interpretation anchors`、`can draft a skill from every activity interval`） |
| 3 | `cd apps/screenpipe-app-tauri && bun run bindings:check` | 通过（本批不应产生 bindings 变化） |
| 4 | 静态证据：`grep -rn "generateActivityHistory" apps/screenpipe-app-tauri/components apps/screenpipe-app-tauri/lib` | 仅测试文件命中 |

## 5. 风险与回退

| 风险 | 处置 |
| --- | --- |
| 停用后时间线在无 DB 摘要的区间变空 | B02b-1 的 KV 回落路径保持；UI 不因缺生成而阻塞 |
| 用户仍想要 AI 叙事 | 命令保留，可在需要时手动触发；物理下线有门禁 |
| 测试删除过多导致覆盖下降 | 只改与自动生成相关的断言，保留其余用例 |

**回退**：恢复被删除的 effect 即可（单文件、单提交）。

## 6. 交付边界

- 允许路径：`apps/screenpipe-app-tauri/components/activity-ledger.tsx`、`components/activity-ledger.test.tsx`、`lib/activity-history-persistence.ts`（仅注释）、`src-tauri/src/activity_history.rs`（仅注释）。
- 禁止：删 Rust 生成器与其测试、改 `lib/utils/tauri.ts`、改 `crates/**`、改 `docs/**`、改 KV 数据结构、修既有失败用例。
- 不 commit / 不 push；真实库只读。
