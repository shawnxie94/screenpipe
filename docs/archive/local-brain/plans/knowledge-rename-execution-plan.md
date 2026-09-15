---
id: plan-knowledge-rename
type: execution_plan
status: approved
created_at: '2026-09-10'
updated_at: '2026-09-10'
sources:
- docs/trd/personal-workbench-integration-framework.md
- docs/roadmap.md
related:
- docs/trd/personal-workbench-integration-framework.md
- docs/plans/personal-brain-correctness-execution-plan.md
base_commit: 206bb07e0
orchestration_mode: batch
execution_target: subagent
execution_backend: zcode_subagent
approval:
  basis: 用户 2026-09-10 确认短名 `knowledge` 并要求开工（框架 D-16 / O-12）。
  scope: 仅代码与接口改名；数据库表名与已应用迁移留待第二批。
---

# knowledge 改名批次（批 1：代码与接口）

对应框架 O-12（短名 `knowledge`）与 P1 收尾遗留项。**纯改名、零行为变化。**

## 1. 范围

| 项 | 改前 | 改后 |
|---|---|---|
| Rust 模块目录 | `crates/screenpipe-engine/src/brain/`、`crates/screenpipe-db/src/db/brain/` | `knowledge/` |
| 模块内同名文件 | `knowledge/knowledge.rs`（改名后会撞名） | `knowledge/items.rs` |
| 函数/方法 | `brain_*`（如 `brain_register_source`、`brain_save_work_unit`） | `knowledge_*` |
| 类型/枚举 | `BrainError`、`BrainJobKind`、`BrainSourceInput`、`TaskKind::BrainExtract/Compile/Backfill` 等 | `Knowledge*` |
| REST 路由 | `/brain/*` | `/knowledge/*` |
| Agent 工具 | `brain_answer`、`brain_source`、`assets/extensions/brain-tools.ts` | `knowledge_*` |
| Tauri | `brain_runtime.rs`、`brain_migration.rs` 及其命令名 | `knowledge_*` |
| 前端 | `components/brain/`、`lib/brain/`、相关类型与测试 | `knowledge/` |
| 测试文件 | `tests/brain_correctness.rs`、`task_migration.rs` 内的 brain 引用 | `knowledge_correctness.rs` 等 |

## 2. 明确不改（第二批）

- 数据库表名（`brain_sources`、`brain_knowledge` 等 43 个）与 `crates/screenpipe-db/src/migrations/` 下**任何已应用迁移内容**。SQL 字符串里的表名保持原样，改名的映射关系写进交付报告。
- 文档（`docs/prd`、`docs/trd`、`docs/plans`、`docs/reviews`）中的引用；改名落地后单独一批处理。
- 任何行为：不得顺手修 bug、不得改 schema、不得增删提示词语义、不得调整测试断言。

## 3. 写归属

允许写：

- `crates/screenpipe-core/src/`
- `crates/screenpipe-db/src/`（除 `migrations/`）
- `crates/screenpipe-engine/src/`、`crates/screenpipe-engine/tests/`
- `crates/screenpipe-connect/src/`、`crates/screenpipe-connect/tests/`（仅限引用改名）
- `apps/screenpipe-app-tauri/src-tauri/src/`、`apps/screenpipe-app-tauri/assets/`、`apps/screenpipe-app-tauri/{components,lib,app}/`、`apps/screenpipe-app-tauri/e2e/`
- `packages/screenpipe-mcp/src/`
- `.agent/tmp/`（写交付报告：改名映射表 + 命令输出摘要）

禁止写：

- `crates/screenpipe-db/src/migrations/**`、`docs/**`、`.git/**`、`Cargo.lock`（除确需改包名，且需在报告说明）
- 发布指针、标签、远程推送；不 commit、不 push、不 `git reset`、不 `git add -A`

## 4. 验收

1. `grep -rn "\bbrain" --include=*.rs crates apps/screenpipe-app-tauri/src-tauri/src` 的剩余命中只允许：数据库表名字符串、`.agent/` 与生成物路径、必要历史注释；**不得有标识符残留**。
2. `grep -rn "\bbrain" --include=*.ts --include=*.tsx apps packages` 同理（只允许表名/接口字符串与注释）。
3. `git diff --stat` 只落在允许路径；`docs/**` 与 `migrations/**` 零改动。
4. 下列命令全绿（输出摘要进报告）：
   - `cargo test -p screenpipe-db --lib knowledge`
   - `cargo test -p screenpipe-engine --lib knowledge`
   - `cargo test -p screenpipe-db --test knowledge_correctness`
   - `cargo test -p screenpipe-engine --test knowledge_correctness --test task_contracts --test task_migration`
   - `cargo test -p screenpipe-connect --lib office`
   - `cd packages/screenpipe-mcp && bun run test && bun run typecheck`
   - `cd apps/screenpipe-app-tauri && bun run test && bun run typecheck`
   - 原生边界：`cd apps/screenpipe-app-tauri && bun run test:tauri`（若机器级构建队列/sccache 不可用，**停并报告 blocked**，不得改用裸 cargo 或 ad-hoc profile 兜底）
5. 交付报告（`.agent/tmp/knowledge-rename-report.md`）：旧名→新名映射表、变更文件数、上述命令的 exit_code 与通过数、未验证项与原因、剩余 `brain` 命中清单及原因。

## 5. 交接

- 协调会话核对验收 1–5 后提交；未通过则带反馈继续同一子任务，不新开任务。
- 批 2（数据库表名 + FTS + 触发器改名的迁移，含备份与副本验证）在批 1 合并后单独出计划。

## 完成记录

- 提交：`800aa2029`
- 交付：152 个文件、+2714/−2714；受保护命名清单见提交说明
- 证据：`docs/reviews/evidence/knowledge-rename-2026-09-10/`（子任务报告 + 协调会话独立复验）
- 验收：协调会话逐条复跑；真实库升级路径用副本 + 仓库自身迁移器验证通过
