---
id: plan-collector-redirection
status: approved
owner: shawn
created: 2026-09-15
updated_at: 2026-09-15
plan_id: plan-collector-redirection
plan_unit_id: root
base_commit: 502b77060
orchestration_mode: batch
execution_target: subagent
execution_backend: zcode_subagent
runtime_adapter: zcode
logical_role: zcode
subagent_role: zcode
subagent_scope: user
selected_subagent_model: builtin:bigmodel-coding-plan/GLM-5.3-Flash
thinking: low
fallback: false
parallel_mode: serial_same_worktree
acceptance_scope: batch
framework: docs/roadmap.md
covers:
  - 方向收口：screenpipe 定义为本地信息收集口（采集 + 接入渠道 + 统一存储/检索），移除知识沉淀/记忆/研究综合等内建产品逻辑
  - office（飞书/腾讯会议）完整功能保留，从 knowledge 域解耦为独立 Connector
  - 移除 knowledge 域代码（engine / db / 前端 / Tauri / MCP / CLI / 活动叙事 / memories）
  - 本地数据目录全量重建；SQLite user_version 迁移链保持连续性
  - 文档与 CI 门禁收敛到新范围
doc-covers: crates/screenpipe-capture, crates/screenpipe-screen, crates/screenpipe-audio, crates/screenpipe-a11y, crates/screenpipe-db/src/db/activity_ledger.rs, crates/screenpipe-db/src/migrations, crates/screenpipe-engine/src/connections_api.rs, crates/screenpipe-connect/src, crates/screenpipe-core/src/tasks, crates/screenpipe-engine/src/tasks
doc-verified: 000000000
---

# 方向收口：screenpipe 定位为本地信息收集口

## 1. 背景与决策

用户 2026-09-15 决策：把全部逻辑整合进一个产品太重、难维护。调整方向：

- **screenpipe = 本地信息收集口**：采集（屏幕/音频/a11y）+ 其他应用数据接入（Connector）+ 统一本地存储与检索。不做知识化加工。
- **不需要记忆**：Local Brain / knowledge 域整体移除，`memories` 一并移除。
- **数据清理**：本地数据目录（`~/.screenpipe`、`~/.screenpipe-dev`）全量重建。
- **飞书/腾讯对接保留**，完整功能（授权、增量游标、对象落库、范围管理）作为"其他应用数据接入"的样板 Connector，从 knowledge 域中解耦。
- **单分支演进**：继续 `zh-local` 分支，不重置历史（`a255ffdf` 提案被否）。
- 文档层已完成（roadmap / README 重写、Local Brain 文档归档，commit `502b77060`）。

## 2. 现状事实（2026-09-15 实测）

### 2.1 待移除对象

| 层 | 位置 | 规模 |
|---|---|---|
| engine 知识逻辑 | `crates/screenpipe-engine/src/knowledge/`（summarize/extract/compile/distill/answer/worker/cadence/office/office_routes/registry/prompts…） | ~20 文件 / 12.4k 行 |
| db 知识逻辑 | `crates/screenpipe-db/src/db/knowledge/`（jobs/work_units/items/distill/deletion/sources/office/history/trace/types…）+ `crates/screenpipe-db/src/db/memories.rs` | 5.8k 行 + 642 行 |
| 前端 | `apps/screenpipe-app-tauri/components/knowledge/`、home 页 `KnowledgeHub`、settings 知识页、`lib/knowledge-search`、`lib/connections` 中 knowledge 引用、导航入口 | 116 文件 |
| Tauri | `src-tauri/src/knowledge_runtime.rs`、`knowledge_views.rs`、`knowledge_migration.rs`、扩展 `knowledge-tools.ts` | — |
| MCP | `packages/screenpipe-mcp/src/index.ts` 知识工具（get-knowledge-source 等） | — |
| CLI skills | `crates/screenpipe-core/assets/skills/knowledge-fetch/`、`knowledge-distill/` 及 screenpipe-api 中知识段 | — |
| 其他引用 | `screenpipe-core`（tasks/agents/pi/chat_control/acp）、`screenpipe-db` knowledge_rename 迁移测试、engine tests | — |

### 2.2 知识数据表（迁移链 `20260907120000_create_brain.sql`）

`brain_state`、`brain_sources`、`brain_source_revisions`、`brain_work_units`、`brain_work_unit_revisions`、`brain_knowledge`、`brain_knowledge_versions`、`brain_rejections`、`brain_dependencies`、`brain_jobs`、`brain_answers`、`brain_feedback`、`brain_deletions`、`brain_cleanup_items`、`brain_tombstones`、`brain_history_entries`、`brain_history_coverage`、`brain_migrations`、`brain_search_documents` 等（含 brain_office_*：`brain_office_connections`、`brain_office_scopes`、`brain_office_objects`、`brain_office_cursors`）。

**office 相关 4 张表保留**（解耦后由 office Connector 自有代码读写），其余知识表全部移除。

### 2.3 office 耦合面（保留功能的改动源头）

- `engine/src/knowledge/office.rs`（36K）：依赖 `super::worker::{JobContext,JobFailure,JobOutcome}`（job 驱动）、`super::sources::register_office_source`、`super::types::{now_iso,format_ts,KnowledgeError}`、`KnowledgeJobKind::OfficeSync`、`knowledge_enqueue_job/cancel_jobs`、`knowledge_invalidate_consumers`（消费方失效，与知识检索耦合）。
- `engine/src/knowledge/office_routes.rs`（6.7K）：目前挂 `/connections/office`（独立于 `/knowledge`），但坐落在 knowledge 目录。
- db 层：连接/scope/cursor/object 全部为 `knowledge_*` 前缀（`KnowledgeOfficeConnectionRow` 等定义在 `db/knowledge/office.rs`）。
- 非 knowledge 消费者（**需要保住**）：`screenpipe-core/src/tasks/mod.rs`、`screenpipe-connect/src/office/types.rs`（独立、清洁）、桌面 `office-connection-card.tsx`、`office_runtime.ts`、`src-tauri/src/office_runtime.rs`、`lib/connections/office.ts`、`lib/connections/office-types.ts`、`settings/connections-section.tsx`。
- `screenpipe-connect/src/office/runner.rs`（独立 CLI runner，清洁，不动）。

### 2.4 保留的采集底座（不动）

`crates/screenpipe-capture|screen|audio|a11y`、`screenpipe-db` 的 `activity_ledger.rs`、`frames.rs`、`audio.rs`、`search.rs`、`accessibility.rs`、`meetings.rs` 等；engine `connections_api.rs`、`tasks`；`screenpipe-connect`（除 knowledge 引用）；桌面 timeline/activity/connections/search 界面；MCP 采查询工具；Pipes。

## 3. 目标与非目标

### 3.1 目标

1. **知识域整体退出**：engine/db/前端/Tauri/MCP/CLI 中 knowledge、memories、活动 AI 叙事全部移除；没有残留代码、路由、工具或界面入口。
2. **office Connector 完整保留**：授权、连接管理、范围(scope)、增量游标、对象落库、手动/自动同步、断开清理，全部功能等价迁移到独立 Connector 模块，不依赖 knowledge worker/jobs。
3. **编译与测试通过**：`cargo test`（受 scope 约束）、`bun test` 全绿；删除引起的图标/路由/类型残留全部解决。
4. **数据全量重建**：清空 `~/.screenpipe-dev`（开发）与 `~/.screenpipe`（生产，用户确认后）。
5. **SQLite 迁移链连续**：保留 `user_version` 递增逻辑；新库从 0 起仍可初始化（迁移文件不能倒退）。
6. **文档与 CI 收敛**：README/roadmap 已改；`check-doc-freshness` 门禁覆盖新路径；`native_tests` 恢复。

### 3.2 非目标

- 开发新 Connector（浏览器/RSS 等）——只在 office 解耦完成后作为下一阶段。
- 重构采集热路径、引入新存储/检索架构。
- 追赶上游 MIT 基线、拆独立分支。
- Windows/Linux 或商业分发。

## 4. 批次与执行顺序

> 关键顺序约束：**先解耦 office → 再删知识域 → 最后重建数据**。office 必须先独立，否则删除 knowledge 会连带拔掉它；数据重建放最后，避免开发中途依赖真实对话语料。

### Round 1：office 解耦为独立 Connector（保住飞书/腾讯）

1. **搬表**：office 4 张表从 `brain_office_*` 迁移脚本脱离 → 独立迁移文件（如 `20260915100000_office_connections.sql`），表名去掉 `brain_` 前缀（`office_connections`、`office_scopes`、`office_objects`、`office_cursors`）。保留 `20260907120000_create_brain.sql` 中表定义不动或随知识移除——**user_version 连续性优先**，新库用新迁移初始化。
2. **db 层**：新建 `screenpipe-db/src/db/office.rs`（或复用现有 `knowledge/office.rs` 并重命名为 `office.rs` 移出 knowledge 目录），把 `knowledge_office_*` 函数改为 `office_*`，`KnowledgeOfficeConnectionRow` → `OfficeConnectionRow`；移除对 `super::` 知识模块的依赖。
3. **engine 层**：`office.rs`/`office_routes.rs` 从 `engine/src/knowledge/` 移到 `engine/src/office/`（新模块）；`OfficeSync` 从 knowledge worker 摘除，改为独立同步调度（复用 `sync_scheduler.rs` 或连接内循环）；`knowledge_invalidate_consumers` 消费方失效逻辑——若只服务知识侧则删除，若 office UI 需要状态同步则改事件/直接查询。
4. **前端**：确认 `connections-section.tsx` / `office-connection-card.tsx` / `office_runtime` 不依赖 knowledge API；knowledge-知识无关路径清理。
5. **验证**：`cargo build` + `cargo test -p screenpipe-db -p screenpipe-engine`（office 相关）；连接虚拟 provider 跑通授权→同步→落库→断开；桌面 office 卡片正常。**提交**。

### Round 2：移除知识域（大删除）

1. **engine**：删除 `engine/src/knowledge/` 整个目录（含临时保留的 office 遗留代码——Round 1 已搬走）；`lib.rs`/`server.rs` 摘除 `knowledge` 模块与 `/knowledge`、`/answer` 路由、`KnowledgeShared` 启动接线；删 `external_memory_sync.rs`（memories 同步）、`retention.rs` 中知识相关、`routes/activity_summary.rs`（活动 AI 叙事）。
2. **db**：删除 `db/knowledge/` 与 `db/memories.rs`；清理迁移链：`20260911120000_rename_brain_to_knowledge.sql` 等知识迁移文件**保留文件本身**（user_version 推进），但其建的知识表在数据重建后不存在——需确认迁移脚本对"表不存在"幂等（若建表失败则新库初始化崩）。**关键：知识表迁移文件要么标注"仅历史"要么随 brain 建表一并删除且 user_version 在别处补足**；以"新库能干净初始化"为验收线。
3. **前端**：删 `components/knowledge/`、home 页 `KnowledgeHub` 与路由映射、settings 知识页、`lib/knowledge-search`、`lib/connections` 中知识残留；导航/Tauri Command 绑定（`knowledge_runtime` 等）清理；`src-tauri/src/knowledge_*.rs` 删除；扩展 `knowledge-tools.ts` 删。
4. **MCP/CLI**：`packages/screenpipe-mcp` 移除知识工具；`screenpipe-core` `assets/skills/knowledge-*` 删，`screenpipe-cli` 提及清理；`agents/pi.rs`、`chat_control.rs`、`acp/runtime.rs`、`window_pattern.rs` 中引用清理。
5. **测试与残留**：`engine/tests/*knowledge*`、`db/tests/*knowledge*` 删除；全仓 grep 无 `knowledge`/`brain_`/`memories`/`work_unit` 残留（允许迁移文件里的历史注释）。
6. **验证**：`cargo build --workspace`（或受 scope 命令）+ `bun run test`；空库启动（brand new DB 初始化走完迁移）；桌面 timeline/search 正常。**提交**。

### Round 3：数据重建与最终收敛

1. 备份旧的 `~/.screenpipe-dev`、`~/.screenpipe`（`mv` 到带日期后缀目录，不直接 rm）。
2. 清空重建（用户已授权全量重建）。启动 dev 闭环，确认从 0 初始化、采集跑起来、知识相关表不存在（或从未创建）。
3. `docs/roadmap.md` 更新 `doc-verified` 到实际提交；`AGENTS.md` 删除/修订知识相关段落；`TESTING.md` 若引用知识回归则修订。
4. `docs/archive/local-brain/` 补一份 README 说明"已退出范围"。
5. 全量 CI：`check-doc-freshness`、`cargo test --workspace --exclude screenpipe-rfdetr-mlx`、`bun run test`。**提交**。

## 5. 验收矩阵

| ID | 验收项 | 判定 |
|---|---|---|
| AC-1 | 无 knowledge/brain/memories 代码残留 | `grep -r knowledge`（排除 `docs/archive/`）≈ 0 |
| AC-2 | 无知识路由 | `/knowledge*`、`/answer`、`/memories*` 全部 404 |
| AC-3 | office 完整功能 | 授权/同步/游标/落库/断开 demo 通过，表 `office_*` 独立 |
| AC-4 | 编译全绿 | `cargo build` + `bun test` 通过（scope 内） |
| AC-5 | 空库初始化 | 全新 `~/.screenpipe-dev` 从 0 建库、跑迁移、启动无错 |
| AC-6 | 采集浏览可用 | 桌面 timeline/search 对真实采集数据正常 |
| AC-7 | doc 门禁 | `check-doc-freshness` 对新 roadmap 覆盖范围通过 |
| AC-8 | 生产库重建 | `~/.screenpipe` 备份 + 清空重建完成（用户在场确认） |

## 6. 风险与对策

| 风险 | 对策 |
|---|---|
| 迁移文件删除破坏 `user_version` 连续性，老库打开崩 | 保留迁移文件本体，仅删建表内容时用"IF NOT EXISTS + 不引用已删类型"的兼容写法；以全新库初始化为主验收线；老库走备份后重建路径 |
| office 解耦过程中功能回退 | Round 1 单独完成并验证后才进 Round 2；`screenpipe-connect` runner 不动；桌面卡片在每步后回归 |
| 删除扩散到共用的 `screenpipe-db` 检索/迁移基础设施 | 先 grep 引用图再删；把 `db/mod.rs` 的 mod 声明逐个核对 |
| `memories` 与 `deletion.rs` 等互引 | Round 2 顺序：先确认 deletion 里 memories 段落随整个 knowledge 目录删除，不单独留 |