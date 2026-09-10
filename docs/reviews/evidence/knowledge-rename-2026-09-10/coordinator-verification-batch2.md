# knowledge 改名批 2 — 协调会话独立复验

- 交付提交：`4b322a531`（批 2 数据库对象与持久化值）
- 基线提交：`800aa2029`（批 1 代码与接口）
- 复验时间：2026-09-10

## 复验结论：通过，但子任务交付里有两处必须由协调会话决定的缺口，已修

### 缺口 1：owner generation 两侧键不一致（批 1 引入，功能回归）

- 桌面运行时批 1 改名为 `task_get_owner_state("knowledge")` / `task_set_owner_state(kind="knowledge")` / `task_activate_owner_generation("knowledge", …)`；
- 遗留持久队列仍按 `task_owner_state WHERE kind='brain'` 读世代（`db/knowledge/jobs.rs:96`），迁移的 `LIKE 'brain\_%'` 也不命中裸 `'brain'` 行；
- 后果：两侧以不同世代做围栏，cutover 激活可能失效。
- 处置：迁移按最大世代把裸 `'brain'` 行合并进 `'knowledge'` 后删除；lookup 同步改为 `'knowledge'`；子任务的迁移测试期望同步更新。

### 缺口 2：cutover 激活 arm 不可达（批 1 引入）

- `task_activate_owner_generation` 的 match arm 键在批 1 后仍为 `"brain"`，而唯一调用方传 `"knowledge"` → 命中 `_ => &[]`，**静默不激活**；
- 处置：arm 键改为 `"knowledge"`，恢复 cutover 批量激活。

### 另外补全：持久化 definition id 归一

计划 §1.4 只要求归一 `task_definitions` 与 `task_owner_state`。`task_runs.definition_id` 与 `task_legacy_map.definition_id` 同样承载 live id（claim 路径按 definition_id 匹配），seed 出来的历史行若不归一将不再被认领；已在迁移中补两条 UPDATE。

## 复验命令与结果

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo test -p screenpipe-db --test knowledge_rename_migration` | 2 passed |
| 2 | `cargo test -p screenpipe-engine --test task_migration` | 6 passed |
| 3 | `cargo test -p screenpipe-db --lib knowledge` / `-p screenpipe-engine --lib knowledge` | 20 / 17 passed |
| 4 | `cargo test -p screenpipe-db --test knowledge_correctness` / engine 同名 | 8 / 13 passed |
| 5 | `cargo test -p screenpipe-engine --test task_contracts` | 2 passed |
| 6 | `cargo test -p screenpipe-connect --lib office` | 19 passed |
| 7 | `cd packages/screenpipe-mcp && bun run test && bun run typecheck` | 92 passed / exit 0 |
| 8 | `cd apps/screenpipe-app-tauri && bun run typecheck` | exit 0 |
| 9 | `cd apps/screenpipe-app-tauri && bun run test` | 24 failed / 4044 passed（与批 1 基线逐项一致，无新增失败） |

## 真实库升级保真验证（新增 `crates/screenpipe-db/tests/legacy_db_upgrade.rs`）

用**副本 + 仓库自身迁移器**（`DatabaseManager::new`，含 sqlite-vec 注册），未动真实库：

| 样本 | 结果 |
|---|---|
| 6 月版真实实例库副本（57MB，`~/Library/Application Support/zhiji-dev/db.sqlite`） | 123 个迁移全跑通；40 个旧对象全部消失、40 个新对象逐一存在；`frames` / `audio_transcriptions` / `ui_events` / `memories` / `meetings` 行数不变；FTS 与投影一致；FK 空、integrity ok |
| e2e 库副本（含裸 `'brain'` owner 行） | 121 个迁移通过，裸 owner 行正确合并为 `'knowledge'` |

- 备份位置：`/tmp/knowledge-rename-backup-2026-09-10/`（`sqlite3 .backup` 热备，源库未受影响）
- 结论：真实库可在新 build 首次启动时自动完成迁移；测试由 `SCREENPIPE_LEGACY_DB` 驱动，未设置时跳过，可长期回归。

## 未验证项

- src-tauri 原生测试未跑（本批无 src-tauri 代码改动）。
- 持久化会话分类值 `brain-task` 未改（存于 SessionRecord，改名需数据迁移），已列入保留清单。
