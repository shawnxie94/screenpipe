# knowledge 数据库改名（批 2）交付报告

- 执行计划：`docs/plans/knowledge-db-rename-execution-plan.md`（source_plan_sha256 = 515741ab565822229a07ab53f69e9f436a3ff96632203ab50dc7a32d82641ede）
- 执行会话：ZCode 子代理，批 2（数据库对象 + 持久化任务值）
- 日期：2026-09-11

## 1. 迁移文件

- 路径：`crates/screenpipe-db/src/migrations/20260911120000_rename_brain_to_knowledge.sql`
- sha256：`cec8bcc2dd79365e55740ed4decb7f1aee304051b257d23e8e6d319267b39d4c`
- 内容（按序）：23 条 `ALTER TABLE ... RENAME TO`；16 个索引 `DROP INDEX` + `CREATE INDEX`（列与 partial WHERE 与原定义逐条一致，含 4 个 UNIQUE）；`DROP TABLE brain_search_fts` 后按原定义新建 `knowledge_search_fts`（fts5, body, doc_id UNINDEXED, tokenize='unicode61'）并从 `knowledge_search_documents` 回填 `(doc_id, body)`；3 条持久化值归一 UPDATE（`task_definitions.kind` / `task_definitions.definition_id` / `task_owner_state.kind`）。
- 事务性：未声明 `no-transaction`，由 sqlx migrator 以单事务执行（默认行为，sqlx 0.9.0，`crates/screenpipe-db/src/db/setup.rs:457` 的 `run_migrations`）。
- 已存在的迁移文件：零改动（`git status --porcelain` 仅有该新增文件为 untracked，其余迁移文件不在变更列表）。

## 2. sqlite_master 新旧对象计数（全新库跑完全部迁移后）

由 `crates/screenpipe-db/tests/knowledge_rename_migration.rs::fresh_database_has_only_knowledge_object_names` 断言：

- 新名对象存在：**40 个**（23 表 + 16 索引 + 1 FTS 虚拟表 `knowledge_search_fts`），逐名断言通过。
- 另有 FTS5 影子表 5 个（`knowledge_search_fts_data/_idx/_content/_docsize/_config`），均为新名派生物。
- 旧对象名（`brain_*` / `idx_brain_*` / 任何含 "brain" 的对象名）：**0 个**（对 sqlite_master 全量行做 `name.contains("brain")` 断言为空）。
- `PRAGMA foreign_key_check`：0 行违规；`PRAGMA integrity_check`：全部 `ok`。

## 3. 验收命令结果（计划 §4 第 5 条 + 新增测试）

| 命令 | exit_code | 结果 |
|---|---|---|
| `cargo test -p screenpipe-db --test knowledge_rename_migration`（新增） | 0 | 2 passed |
| `cargo test -p screenpipe-db --lib knowledge` | 0 | 20 passed, 0 failed |
| `cargo test -p screenpipe-db --test knowledge_correctness` | 0 | 8 passed, 0 failed |
| `cargo test -p screenpipe-engine --lib knowledge` | 0 | 17 passed, 0 failed |
| `cargo test -p screenpipe-engine --test knowledge_correctness --test task_contracts --test task_migration` | 0 | 13 + 2 + 6 = 21 passed, 0 failed |
| `cargo test -p screenpipe-connect --lib office` | 0 | 19 passed, 0 failed |
| `cd packages/screenpipe-mcp && bun run test` | 0 | 14 files / 92 tests passed |
| `cd packages/screenpipe-mcp && bun run typecheck` | 0 | 通过 |
| `cd apps/screenpipe-app-tauri && bun run typecheck` | 0 | 通过 |
| 附加：`bun x vitest run lib/dev/browser-runtime.test.ts components/__tests__/chat-sidebar-archive-all.test.tsx`（app 目录，覆盖本次前端改动） | 0 | 18 passed |

归一与 FTS 验证由 `rename_migration_normalizes_legacy_values_and_rebuilds_fts` 覆盖：以 4 个历史迁移 SQL 重建改名前 schema，预置 `task_definitions.definition_id='brain.extract'/kind='brain_extract'`（seed 自带）与 `task_owner_state.kind='brain_extract'` 及裸 `'brain'` 行，执行新迁移后断言：定义归一为 `knowledge.*`/`knowledge_*`（`office.sync`/`office_sync` 不变），owner 归一为 `knowledge_extract` 且裸 `'brain'` 保留；`knowledge_jobs` 数据存活；FTS 行数 == `knowledge_search_documents` 行数（2==2），`MATCH 'needle'` 精确命中 doc-1；FK/integrity 通过。

## 4. 代码侧改动

- SQL 字符串全部切换（23 表名 + 16 索引名不在代码中出现 + FTS 名，含 `bm25(knowledge_search_fts, 10.0)`、`MATCH`、DELETE/INSERT 目标）：`crates/screenpipe-db/src/db/knowledge/*`、`db/tasks/mod.rs`、`db/maintenance.rs`、`crates/screenpipe-engine/src/knowledge/*`、`src/tasks/mod.rs`、`crates/screenpipe-connect/src/office/types.rs` 及对应 tests。逐条替换采用最长优先的精确字符串匹配（`brain_knowledge_versions` 先于 `brain_knowledge` 等），`brain_jobs` 仅替换 `FROM/INTO/UPDATE` 三种 SQL 上下文。
- 持久化任务值常量切换（与迁移归一后的新值对齐）：`task_binding`（db/jobs.rs）、`definition_for_kind`（engine/worker.rs）、`register_default_definitions` 与 `is_managed_definition_id`（engine/tasks/mod.rs）、`task_activate_owner_generation` 定义列表（db/tasks/mod.rs）、`engine/tests/task_migration.rs:252` 认领列表。
- 会话内部分类前缀：写入侧（`src-tauri/src/knowledge_runtime.rs:173`，批 1 已用 `__title:knowledge-`）不变；读取侧 `apps/screenpipe-app-tauri/lib/utils/internal-session.ts` 改为双前缀识别（新 `__title:knowledge-` + 旧 `__title:brain-` 作 `INTERNAL_KNOWLEDGE_TASK_LEGACY_PREFIX` 保留），分类值 `brain-task` 不变（该值持久化于 SessionRecord.internalCategory，按该文件约定改名需迁移，超出本批范围），历史会话仍归入知识沉淀分组。
- 前端 mock fixture 对齐引擎新值：`lib/dev/browser-engine-mock.ts`、`lib/dev/browser-runtime.test.ts` 中 `brain.extract` → `knowledge.extract`。
- 注释同步：非 legacy 的文档注释中表名更新为新名（如 `types.rs` `knowledge_migrations.state`、`connect/office/types.rs` `knowledge_office_scopes`、engine `deletion.rs`、`worker.rs:577`）；`worker.rs:178` 描述 legacy `brain_jobs` 命名空间的注释按保留清单不动。

## 5. 保留标识清单（未改，均为历史事实/幂等键/第三方）

1. `task_legacy_map.legacy_namespace = 'brain_jobs'`：`db/knowledge/jobs.rs:129,153,157`、`engine/src/tasks/mod.rs:798`、`engine/tests/task_migration.rs:37`。
2. 已导入行 `run_id = 'brain-job-<id>'`：`db/knowledge/jobs.rs`（16 处 format!/SQL 拼接）、`db/tasks/mod.rs:618` `strip_prefix("brain-job-")`、`engine/tasks/mod.rs:796`、`engine/knowledge/worker.rs:457,502,519`、`engine/tests/task_migration.rs:42,368`。
3. `task_owner_state` 裸 kind `'brain'`（legacy owner 世代键，不带下划线，迁移 LIKE 不命中）：`db/knowledge/jobs.rs:96`、`db/tasks/mod.rs:878`、`engine/tests/task_migration.rs:113,125`。
4. `live_views.rs` 的 `LEGACY_CONSUMER_ID = 'desktop.brain-overview.v1'` 及 `brain-overview-views.json` 文件名；`structured_outputs.rs`/`routes/structured_outputs.rs` 的 `desktop.brain-overview.v1`、`brain/main/*` 测试 fixture；`crates/screenpipe-core/src/pipes/permissions.rs` 的 `desktop.brain-overview:*` 测试路径。
5. 第三方/无关别名：`braintree`（前端 URL 基准数据）、`brainstorming`（ai-prompt-journal 管道分类词）、lucide 图标 `brain-cog`/`brain-circuit`、`~/brain` 用户路径样例、e2e 历史结果文件（`e2e/results/*brain*.json`、`coverage-map.json`、`COVERAGE.md` 历史描述）。

## 6. 未验证项与风险（交协调会话）

1. **真实用户库**：按边界未做任何真实库操作。备份、副本验证、执行由协调会话负责。
2. **task_runs/task_legacy_map 已导入行的 definition_id 仍为 `brain.extract` 等**：计划 §1.4 明确只归一 `task_definitions` 与 `task_owner_state`，故未加 UPDATE。迁移后 public worker 以 `knowledge.*` 认领，历史导入的 queued 行（若有在途）将不再被认领。真实库迁移前建议协调会话确认无在途导入行或自行决定是否补归一。
3. **`task_activate_owner_generation` 的 `"brain"` match arm 在批 1 后已不可达**（唯一调用方 `knowledge_runtime.rs:563` 传 `"knowledge"`）。本批按零行为变化原则仅更新了 arm 内 definition_id 字符串，未改变其不可达状态；是否改为 `"knowledge"`（会激活 cutover 批量标注）属行为变更，留协调会话决策。
4. 前端全量 `bun run test` 未跑（§4 只要求 typecheck；全量基线含 24 个既有失败），改为定向运行了覆盖本次改动文件的 2 个测试文件（18 passed）。
5. src-tauri 原生测试未跑：本次无 src-tauri 代码改动（`knowledge_runtime.rs` 批 1 已写新前缀），§4 亦未要求。
6. `crates/screenpipe-db` lib 的 7 个 dead-code warning（如 `LOCAL_DATASET_ID`、`MigrationPhase`）为批 1 遗留的既有告警，与本批无关，未处理。

## 7. 自检

- `git status --porcelain`：28 个修改文件全部位于允许路径内；新增 2 个文件（迁移 SQL + 测试）；`docs/**` 与已存在的迁移文件零改动；无 commit/push。
- 计划 §1 的 40 个对象映射逐条执行，两个特例（`brain_knowledge`→`knowledge_items`、`brain_knowledge_versions`→`knowledge_item_versions`、`idx_brain_kv_state`→`idx_knowledge_item_state`）已按特例名落地。
