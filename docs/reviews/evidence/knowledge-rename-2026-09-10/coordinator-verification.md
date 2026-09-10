# knowledge 改名批 1 — 协调会话独立复验

- 交付提交：`800aa2029`（批 1 代码与接口改名）
- 复验时间：2026-09-10
- 复验人：协调会话（不采信子任务自述，逐条重跑）

## 复验命令与结果

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo check -p screenpipe-core -p screenpipe-db -p screenpipe-engine -p screenpipe-connect` | exit 0（仅既有 warning） |
| 2 | `cargo test -p screenpipe-db --lib knowledge` | 20 passed / 0 failed |
| 3 | `cargo test -p screenpipe-engine --lib knowledge` | 17 passed / 0 failed |
| 4 | `cargo test -p screenpipe-db --test knowledge_correctness` | 8 passed / 0 failed |
| 5 | `cargo test -p screenpipe-engine --test knowledge_correctness` | 13 passed / 0 failed |
| 6 | `cargo test -p screenpipe-engine --test task_contracts` | 2 passed / 0 failed |
| 7 | `cargo test -p screenpipe-engine --test task_migration` | 6 passed / 0 failed |
| 8 | `cd packages/screenpipe-mcp && bun run test && bun run typecheck` | 14 files / 92 passed；typecheck exit 0 |
| 9 | `cd apps/screenpipe-app-tauri && bun run typecheck` | exit 0 |
| 10 | `bun run bindings:generate` 两次比对 | 生成前/后 `lib/utils/tauri.ts` 哈希一致（`c19f2989…`）→ 幂等，bindings 过期问题闭合 |
| 11 | `bun run test`（前端全量） | 24 failed / 4044 passed，与子任务报告一致 |
| 12 | 残留审计：`grep -rho "\bbrain[a-z_.-]*"`（Rust）与 `--include=*.ts`（前端+MCP） | 仅剩受保护对象，见下 |

## 24 个前端失败的既有性判定（独立复核）

- 9 个失败文件中 8 个**本次改名未触碰**（`git status` 无记录）：system-prompt、chat-chart、chart-markdown、receipts、activity-ledger、automation-pipe-evals、native-timeline、summarize-with-ai。
- 第 9 个 `lib/live-views/__tests__/item-actions.test.ts` 只被改了类型名（`BrainViewSlot` → `KnowledgeViewSlot`），运行期被擦除；其失败原因是断言写英文 `"Ask me to confirm the exact destination before sending anything."`，而实现早已中文化（`发送前请让我确认确切目标。`）——属既有中英文案脱节。
- 结论：24 个失败与本批次无关，未修复（超出授权，属文案/测试批次）。

## 受保护命名（改名会改行为或需数据迁移）

数据库对象 `brain_*`（40 个）；持久化值 `brain.extract` / `brain.compile` / `brain.backfill`（已应用迁移 seed 的 definition id）、`brain-job-*`（legacy run id 映射）、`brain-task`（会话内部分类，持久化）；桌面输出目标 `desktop.brain-overview`；第三方别名 `braintree` / `brainfuck` / lucide 图标 `brain-cog`、`brain-circuit`。以上留给批 2 或永久保留，已写入批 1 提交说明。

## 子任务护栏失败的原因（流程教训）

子任务以 `scope_violation` 结束，与该批代码质量无关，原因有二：

1. 协调会话给出的 `implementation_paths` 过窄：漏了 `crates/screenpipe-db/tests/` 与 `apps/screenpipe-app-tauri/src-tauri/assets/`，导致合法产物被判越界。
2. 协调会话在任务运行期间提交了计划文件（HEAD 变更），使计划文件被计为越界。

后续委派：写归属必须覆盖测试目录与 `src-tauri/assets/`；任务运行期间不提交、不改 HEAD。
