---
id: plan-knowledge-db-rename
type: execution_plan
status: approved
created_at: '2026-09-10'
updated_at: '2026-09-10'
sources:
- docs/trd/personal-workbench-integration-framework.md
- docs/plans/knowledge-rename-execution-plan.md
- docs/reviews/evidence/knowledge-rename-2026-09-10/subagent-report.md
related:
- docs/plans/knowledge-rename-execution-plan.md
base_commit: 4fe044ece
orchestration_mode: batch
execution_target: subagent
execution_backend: zcode_subagent
approval:
  basis: 用户 2026-09-10 确认短名 `knowledge` 并授权协调会话在真实库上执行迁移（先备份 + 副本验证）。
  scope: 批 2 = 数据库对象改名 + 持久化任务值归一 + FTS 重建；真实库的备份与执行由协调会话负责。
---

# knowledge 改名批次（批 2：数据库与持久化值）

批 1（代码与接口，提交 `800aa2029`）已完成。本批把**数据库对象**与**持久化值**改为新名，并保持行为不变。

## 1. 迁移文件（新建，禁止改已应用迁移）

在 `crates/screenpipe-db/src/migrations/` 新增一个 `YYYYMMDDHHMMSS_rename_brain_to_knowledge.sql`，内容按序：

1. **重命名 23 张表**：`ALTER TABLE <旧> RENAME TO <新>;`（SQLite 会同步更新其它表的外键引用）。
2. **重建 16 个索引**：SQLite 不支持改索引名，需 `DROP INDEX <旧>; CREATE INDEX/UNIQUE INDEX <新> ...;`，列与条件与原定义完全一致（逐条比对 `20260907120000_create_brain.sql` 与后续增量迁移）。
3. **重建全文索引**：`DROP TABLE brain_search_fts;` 后按原定义新建 `knowledge_search_fts USING fts5(body, doc_id UNINDEXED, tokenize='unicode61')`，再从 `knowledge_search_documents` 回填：`INSERT INTO knowledge_search_fts(doc_id, body) SELECT doc_id, body FROM knowledge_search_documents;`（该索引是派生数据，可重建）。
4. **持久化任务值归一**（同事务）：
   - `UPDATE task_definitions SET kind = replace(kind,'brain_','knowledge_') WHERE kind LIKE 'brain\_%' ESCAPE '\\';`
   - `UPDATE task_definitions SET definition_id = replace(definition_id,'brain.','knowledge.') WHERE definition_id LIKE 'brain.%';`
   - `UPDATE task_owner_state SET kind = replace(kind,'brain_','knowledge_') WHERE kind LIKE 'brain\_%' ESCAPE '\\';`
5. 迁移**必须在一个事务内**、不得声明 `no-transaction`；结束后不得残留可改名的 `brain_*` 对象。

### 改名映射（40 个对象，逐条执行）

| 旧 | 新 |
|---|---|
| `brain_answers` | `knowledge_answers` |
| `brain_cleanup_items` | `knowledge_cleanup_items` |
| `brain_deletions` | `knowledge_deletions` |
| `brain_dependencies` | `knowledge_dependencies` |
| `brain_feedback` | `knowledge_feedback` |
| `brain_history_coverage` | `knowledge_history_coverage` |
| `brain_history_entries` | `knowledge_history_entries` |
| `brain_jobs` | `knowledge_jobs` |
| `brain_knowledge` | `knowledge_items` |
| `brain_knowledge_versions` | `knowledge_item_versions` |
| `brain_migrations` | `knowledge_migrations` |
| `brain_office_connections` | `knowledge_office_connections` |
| `brain_office_cursors` | `knowledge_office_cursors` |
| `brain_office_objects` | `knowledge_office_objects` |
| `brain_office_scopes` | `knowledge_office_scopes` |
| `brain_rejections` | `knowledge_rejections` |
| `brain_search_documents` | `knowledge_search_documents` |
| `brain_search_fts` | `knowledge_search_fts` |
| `brain_source_revisions` | `knowledge_source_revisions` |
| `brain_sources` | `knowledge_sources` |
| `brain_state` | `knowledge_state` |
| `brain_tombstones` | `knowledge_tombstones` |
| `brain_work_unit_revisions` | `knowledge_work_unit_revisions` |
| `brain_work_units` | `knowledge_work_units` |
| `idx_brain_cleanup_deletion` | `idx_knowledge_cleanup_deletion` |
| `idx_brain_deletions_journal_seq` | `idx_knowledge_deletions_journal_seq` |
| `idx_brain_deps_consumer` | `idx_knowledge_deps_consumer` |
| `idx_brain_deps_source` | `idx_knowledge_deps_source` |
| `idx_brain_feedback_target` | `idx_knowledge_feedback_target` |
| `idx_brain_history_coverage_span` | `idx_knowledge_history_coverage_span` |
| `idx_brain_jobs_active_input` | `idx_knowledge_jobs_active_input` |
| `idx_brain_jobs_batch` | `idx_knowledge_jobs_batch` |
| `idx_brain_jobs_claim` | `idx_knowledge_jobs_claim` |
| `idx_brain_kv_state` | `idx_knowledge_item_state` |
| `idx_brain_search_docs_ref` | `idx_knowledge_search_docs_ref` |
| `idx_brain_sources_captured` | `idx_knowledge_sources_captured` |
| `idx_brain_sources_locator` | `idx_knowledge_sources_locator` |
| `idx_brain_sources_office` | `idx_knowledge_sources_office` |
| `idx_brain_sources_state` | `idx_knowledge_sources_state` |
| `idx_brain_wu_scope` | `idx_knowledge_wu_scope` |

## 2. 代码侧 SQL 字符串

批 1 保留了表名字符串，本批全部切换。涉及 `crates/screenpipe-db/src/db/knowledge/*`、`db/tasks/mod.rs`、`db/maintenance.rs`、`crates/screenpipe-engine/src/knowledge/*`、`src/tasks/mod.rs`、`crates/screenpipe-engine/tests/*`、`crates/screenpipe-db/tests/*` 等（以 `grep -rn "brain_" --include=*.rs` 为准，逐条替换为新名，不做批量正则误伤）。

## 3. 明确保留（历史标识，勿改）

- `task_legacy_map.legacy_namespace = 'brain_jobs'`、已导入行的 `task_runs.run_id = 'brain-job-<id>'`：描述**改名前的旧表**，是历史事实与幂等键。
- `desktop.brain-overview.v1`（`live_views.rs` 的 `LEGACY_CONSUMER_ID`）：旧输出消费者 id 的兼容读取入口。
- 第三方别名：`braintree`、`brainfuck`、lucide 图标 `brain-cog` / `brain-circuit`。
- 会话内部分类前缀：新写入用新前缀，**读取同时兼容旧的 `brain-` 前缀**（否则历史会话会掉出知识沉淀分组）。

## 4. 验收

1. 全新库跑完全部迁移后：`sqlite_master` 中存在全部 40 个新名对象，且不存在任何旧的 `brain_*`/`idx_brain_*` 对象名。
2. `PRAGMA foreign_key_check` 为空；`PRAGMA integrity_check` 为 ok。
3. 数据迁移正确：预置旧值（`task_definitions.kind='brain_extract'`、`definition_id='brain.extract'`、`task_owner_state.kind='brain_extract'`）后跑迁移，断言全部归一为新值。
4. FTS 可查：插入搜索文档后 `knowledge_search_fts MATCH` 能命中，行数与 `knowledge_search_documents` 一致。
5. 测试全绿：`cargo test -p screenpipe-db --lib knowledge`、`cargo test -p screenpipe-engine --lib knowledge`、`cargo test -p screenpipe-db --test knowledge_correctness`、`cargo test -p screenpipe-engine --test knowledge_correctness --test task_contracts --test task_migration`、`cargo test -p screenpipe-connect --lib office`、`cd packages/screenpipe-mcp && bun run test && bun run typecheck`、`cd apps/screenpipe-app-tauri && bun run typecheck`。
6. 交付报告 `.agent/tmp/knowledge-db-rename-report.md`：迁移文件路径与 sha256、每条验收命令的 exit_code 与结果、`sqlite_master` 新旧对象计数、保留标识清单、未验证项。

## 5. 边界

- **禁止**：修改 `crates/screenpipe-db/src/migrations/` 下**已存在**的迁移；改动 `docs/**`；commit / push；直接对真实用户库执行任何操作（真实库的备份、副本验证与执行由协调会话负责）。
- 行为零变化：除对象名与上述持久化值归一外，不改任何逻辑、提示词、断言含义。

## 完成记录

- 提交：`4b322a531`
- 交付：23 表 + 16 索引 + FTS 重建 + 持久化值归一；协调会话补修 owner generation 键不一致与不可达的 cutover arm
- 证据：`docs/reviews/evidence/knowledge-rename-2026-09-10/`（子任务报告 + 协调会话独立复验）
- 验收：协调会话逐条复跑；真实库升级路径用副本 + 仓库自身迁移器验证通过
