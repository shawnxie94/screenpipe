# knowledge 改名批次（批 1：代码与接口）交付报告

- 计划：`docs/plans/knowledge-rename-execution-plan.md`（source_plan_sha256 = 1c0ae19c2408823805223bed0c3966638dcb6caefd5a2b3c50d2b31478cc114b）
- 基线：`206bb07e0`（与计划 base_commit 一致，分支 `zh-local`）
- 性质：纯改名、零行为变化；不 commit、不 push
- 变更规模：**150 个文件，+2694 / −2694 行（完全对称）**，另含 25 组 `git mv`（Rust 9 + TS 16）

## 1. 旧名 → 新名映射表

### 1.1 目录 / 文件（git mv）

| 旧 | 新 |
|---|---|
| `crates/screenpipe-engine/src/brain/` | `crates/screenpipe-engine/src/knowledge/` |
| `crates/screenpipe-engine/src/brain/knowledge.rs` | `crates/screenpipe-engine/src/knowledge/items.rs` |
| `crates/screenpipe-db/src/db/brain/` | `crates/screenpipe-db/src/db/knowledge/` |
| `crates/screenpipe-db/src/db/brain/knowledge.rs` | `crates/screenpipe-db/src/db/knowledge/items.rs` |
| `crates/screenpipe-db/tests/brain_correctness.rs` | `crates/screenpipe-db/tests/knowledge_correctness.rs` |
| `crates/screenpipe-engine/tests/brain_correctness.rs` | `crates/screenpipe-engine/tests/knowledge_correctness.rs` |
| `apps/.../src-tauri/src/brain_runtime.rs` | `knowledge_runtime.rs` |
| `apps/.../src-tauri/src/brain_migration.rs` | `knowledge_migration.rs` |
| `apps/.../src-tauri/src/brain_views.rs` | `knowledge_views.rs` |
| `apps/.../src-tauri/assets/extensions/brain-tools.ts` | `knowledge-tools.ts`（含 `__tests__/knowledge-tools.test.ts`） |
| `apps/.../components/brain/` | `components/knowledge/` |
| `apps/.../lib/brain/` | `lib/knowledge/` |
| `components/settings/brain-section.tsx` / `brain-overview.tsx` | `knowledge-section.tsx` / `knowledge-overview.tsx`（含对应 `__tests__`） |
| `components/__tests__/local-brain-foundation.test.tsx` | `local-knowledge-foundation.test.tsx` |
| `lib/utils/brain-search.ts` / `.test.ts` | `knowledge-search.ts` / `.test.ts` |
| `e2e/specs/brain-overview.spec.ts` / `brain-section.spec.ts` / `local-brain-entry-split.spec.ts` | `knowledge-overview.spec.ts` / `knowledge-section.spec.ts` / `local-knowledge-entry-split.spec.ts` |
| `e2e/screenshots/brain-process-map-template.png` | `knowledge-process-map-template.png`（无代码引用，仅 e2e 基线图） |

### 1.2 Rust 标识符

| 旧 | 新 | 说明 |
|---|---|---|
| `mod brain` / `crate::brain` / `screenpipe_engine::brain` / `db::brain` | `…::knowledge` | 模块路径，`pub use self::brain::…` 同步 |
| `BrainShared` / `BrainModelExecutor` / `BrainError` / `BrainJobKind` / `BrainSourceInput` / `BrainSearchHit` / `BrainReviewUpdate` / `ClaimedBrainJob` / `BrainSourceRegistration/Row` / `BrainSearchDocInput` 等 | `Knowledge*` | 全部类型/结构/枚举 |
| `TaskKind::BrainExtract / BrainCompile / BrainBackfill` | `KnowledgeExtract / KnowledgeCompile / KnowledgeBackfill` | 枚举变体 |
| TaskKind 序列化串 `"brain_extract"` / `"brain_compile"` / `"brain_backfill"` | `"knowledge_extract"` 等 | 计划点名；`as_str`/`from_str`/`db/knowledge/jobs.rs` bind 点与 TS 手抄契约同步 |
| `brain_register_source`、`brain_save_work_unit`、`brain_enqueue_job`、`brain_claim_next_job`、`brain_current_deletion_epoch` 等约 200 个 `brain_*` 方法/函数/字段 | `knowledge_*` | 含 `brain_dir`、`brain_chinese_tests` mod |
| `…_brain_*` / `…_brain` 中段与结尾形态（`start_brain_worker`、`stop_brain_worker`、`list_brain_views`、`delete_brain_view`、`ensure_brain_tools_extension`、`classify_brain_event`、`activate_public_brain_owner`、`mirror_new_entries_to_brain` 等） | `…_knowledge_*` | 第二轮补漏（`\b` 词边界在 `_` 后不成立，首轮漏 103 处） |
| REST 路由 `"/brain"`、`/brain/*` | `"/knowledge"`、`/knowledge/*` | engine `nest`、`lib/knowledge/api.ts BASE`、mock 正则、mcp 调用 |
| Tauri 事件 `"open-brain-artifact"` | `"open-knowledge-artifact"` | Rust emit 与 TS `OPEN_KNOWLEDGE_ARTIFACT_EVENT` 两端同步 |
| Tauri 命令 `list_brain_views` 等 7 个 | `list_knowledge_views` 等 | Rust `#[tauri::command]` 与前端 `TAURI_INVOKE`/mock 同步 |

### 1.3 TS / 前端

| 旧 | 新 |
|---|---|
| `brain_answer` / `brain_source`（agent 工具名） | `knowledge_answer` / `knowledge_source` |
| mcp 工具 `get-brain-source` | `get-knowledge-source` |
| `lib/tasks/types.ts` kind 联合类型 `"brain_extract" \| "brain_compile" \| "brain_backfill"` | `"knowledge_extract" \| …` |
| `BrainSection` / `BrainOverview` / `BrainViewState` 等组件/类型 | `KnowledgeSection` / `KnowledgeOverview` / `KnowledgeViewState` |
| `brainViewState`、`brainState`、`mockBrainResponse` 等 camelCase | `knowledgeViewState` … |
| `SECOND_BRAIN_PROMPT` / `INTERNAL_BRAIN_TASK_PREFIX` / `BRAIN_HANDOFF_STORAGE_KEY` / `ONBOARDING_BRAIN_HANDOFF_EVENT` | `SECOND_KNOWLEDGE_PROMPT` 等（其**值**均不含 brain，或值两端成对更新） |
| UI 文案/注释中的领域词（"Brain 沉淀"、"Local Brain"、"the Brain view" 等） | "Knowledge 沉淀" 等 |

### 1.4 第三方库导出名（不改，已恢复）

lucide-react 图标 `Brain` / `BrainCircuit` 是**库导出名**，不属于改名范围；首轮误改后在 14 个文件恢复（含 import 与 JSX 用法；注释中指图标名的 "Brain icon" 一并恢复）。

## 2. 明确保留（批 2 / 非领域标识），即"剩余 brain 命中清单"

### 2.1 数据库对象名（40 个，批 2 处理）

`brain_answers, brain_cleanup_deletion, brain_cleanup_items, brain_deletions, brain_deletions_journal_seq, brain_dependencies, brain_deps_consumer, brain_deps_source, brain_feedback, brain_feedback_target, brain_history_coverage, brain_history_coverage_span, brain_history_entries, brain_jobs, brain_jobs_active_input, brain_jobs_batch, brain_jobs_claim, brain_knowledge, brain_knowledge_versions, brain_kv_state, brain_migrations, brain_office_connections, brain_office_cursors, brain_office_objects, brain_office_scopes, brain_rejections, brain_search_docs_ref, brain_search_documents, brain_search_fts, brain_source_revisions, brain_sources, brain_sources_captured, brain_sources_locator, brain_sources_office, brain_sources_state, brain_state, brain_tombstones, brain_work_unit_revisions, brain_work_units, brain_wu_scope`

出现在 Rust SQL 字符串（`db/knowledge/*`、`db/maintenance.rs`、`db/tasks/mod.rs`、`text_normalizer.rs` 注释等），运行时字符串字面量，零改动。

### 2.2 持久化值 / 与已应用迁移 seed 耦合的运行时字符串（不改；改了=行为变化）

| 值 | 出现点 | 原因 |
|---|---|---|
| `"brain.extract"` / `"brain.compile"` / `"brain.backfill"` | `db/tasks/mod.rs:879-881`、`engine/tasks/mod.rs:120-130,710`、`db/knowledge/jobs.rs` `task_binding`、`engine/knowledge/worker.rs`、TS mock、`task_migration.rs:252` | task_definitions 主键（definition_id）。迁移 `20260908150000_create_tasks.sql` 以 `INSERT OR IGNORE` 写入同名行；运行时按其匹配 claim/activate。改名将产生孤儿 seed 行并使既有 queued runs 不可见 |
| `kind='brain'` / `"brain".into()` / `"brain" =>` | `db/knowledge/jobs.rs:96`、`db/tasks/mod.rs:878`、`task_migration.rs:113,125` | task_owner_state 主键值，seed 与运行时耦合 |
| `brain-job-`（run_id 前缀） | `db/knowledge/jobs.rs`×8、`db/tasks/mod.rs:618`、`engine/tasks/mod.rs:796`、seed SQL `'brain-job-' \|\| id`、`task_migration.rs` 断言 | 同上 |
| `"brain_jobs"`（legacy_namespace） | `engine/tasks/mod.rs:798`、`task_migration.rs:37`、`db/knowledge/jobs.rs` | `task_legacy_map` 查询键 |
| `desktop.brain-overview.v1`、`desktop.brain-overview:main:focus-time`、`brain/main/focus`、`brain/main/stale`、`brain-overview-views.json`、`desktop.brain/main/*` | `engine/live_views.rs`、`engine/structured_outputs.rs`（测试 fixture）、`core/pipes/permissions.rs`（测试 fixture） | 持久化 live-view consumer/target id 与磁盘文件名 |
| `` `${INTERNAL_TITLE_PREFIX}brain-` `` 与分类值 `"brain-task"`（含 testid `sidebar-subsection-brain-task`） | `lib/utils/internal-session.ts`、`pi-event-router.ts`、`chat-store.ts`、chat-sidebar | 从已存在会话标题解析，改名会使旧会话分类失效 |
| `brain-runner-{uuid}` | `screenpipe-connect` 测试临时目录 | 随机临时目录名，非领域接口（未动） |

### 2.3 非领域命中（不动）

- `screenpipe-audio`（bench/accuracy 参考文本 "brain muscles"）、`screenpipe-screen`（窗口标题 fixture "π - brain"）：测试样例数据，且这两个 crate 不在允许写路径。
- `lib/brand.ts:9` 注释引用 `docs/reviews/personal-brain-brand-retained-identifiers.md`：文档名，文档批后续处理。
- `docs/**`、`migrations/**`：按计划零改动（`git diff` 验证为 0）。

## 3. 验证命令与结果（计划 §4）

| # | 命令 | exit_code | 结果 |
|---|---|---|---|
| 1 | `cargo test -p screenpipe-db --lib knowledge` | 0 | **20 passed**, 0 failed |
| 2 | `cargo test -p screenpipe-engine --lib knowledge` | 0 | **17 passed**, 0 failed |
| 3 | `cargo test -p screenpipe-db --test knowledge_correctness` | 0 | **8 passed**, 0 failed |
| 4 | `cargo test -p screenpipe-engine --test knowledge_correctness --test task_contracts --test task_migration` | 0 | **13 + 2 + 6 passed**, 0 failed |
| 5 | `cargo test -p screenpipe-connect --lib office` | 0 | **19 passed**, 0 failed |
| 6a | `cd packages/screenpipe-mcp && bun run test` | 0 | **92 passed** (14 files) |
| 6b | `cd packages/screenpipe-mcp && bun run typecheck` | 0 | clean |
| 7a | `cd apps/screenpipe-app-tauri && bun run typecheck` | 0 | clean（修复 lucide 图标误改后） |
| 7b | `cd apps/screenpipe-app-tauri && bun run test` | **1** | 4044 passed / **24 failed** —— 24 个失败全部为**既有失败**，见 §3.1 |
| 8 | `cd apps/screenpipe-app-tauri && bun run test:tauri` | 首跑 101 → 再生 bindings 后复跑通过 | 685 passed；唯一失败 `specta_bindings::tests::tauri_bindings_are_current`（命令改名后 checked-in bindings 过期），执行 `bun run bindings:generate` 重新生成 `lib/utils/tauri.ts` 后复跑全绿，见 §3.2 |

辅助验证：`cargo check -p screenpipe-db -p screenpipe-core -p screenpipe-engine -p screenpipe-connect` 全部通过。

### 3.1 前端 24 个失败测试的既有性证明

失败分布：`system-prompt.test.ts` ×14、`chat-chart` / `chart-markdown` ×2、`receipts` ×2、`activity-ledger` ×2、`automation-pipe-evals` ×2、`native-timeline` ×1、`summarize-with-ai.session` ×1。

证据：
1. 这些测试的**被测源文件与测试文件均不在本次 150 文件改动集内**（`git diff --name-only` 核对；唯一交集 `summarize-with-ai.test.ts` 的 diff 仅 fixture 窗口标题/路径改名）。
2. 在 HEAD 基线 `206bb07e0` 上直接验证脱节已存在：`git show 206bb07e0:.../lib/chat/system-prompt.ts` 不含测试要求的 `# Voice and length`（grep 计数 0）；chart 断言 `Add chart to a Live View` 在 HEAD 的 chart 源文件中计数 0（文案已中文化而断言仍是英文）。失败特征（中英文案脱节、prompt 快照缺失小节）与 brain/knowledge 无关。
3. `receipts.tsx`、`activity-ledger.tsx`、`automation-pipe-evals.ts`、`native-timeline.tsx`、`summarize-with-ai.session.test.ts` 的 import 链不触及任何被改模块（逐文件 grep 验证）。

结论：改名前即红；按"不顺手修 bug、不改断言语义"约束不予处理，留待文案/测试批次收口。

### 3.2 原生边界（test:tauri）与 bindings

- 首跑 `bun run test:tauri`：**685 passed / 1 failed**。唯一失败 `specta_bindings::tests::tauri_bindings_are_current`——Tauri 命令改名后，checked-in 的 specta TypeScript bindings（输出路径即 `lib/utils/tauri.ts`，`default_bindings_path()`）过期。
- 处置：按仓库机制执行 `bun run bindings:generate`（走机器级构建队列，slot 1m00s）重新生成；diff 显示仅为命令包装改名（`deleteBrainView→deleteKnowledgeView` 等）与按注册顺序重排，类型 `BrainViewDefinition→KnowledgeViewDefinition` 同步，生成后 `tauri.ts` 零 brain 残留。
- 复跑 `bun run test:tauri`：**686 passed / 0 failed / 4 ignored**（队列 slot 9s，增量编译）。
- `src-tauri/gen/schemas/**` 无 diff（`git status` 核对），无生成噪音需要还原。
- bindings 再生成后复跑 `bun run test`：24 failed / 4044 passed，与改名前完全一致，无新增失败。

## 4. 行为零变化说明与风险

1. **schema / 迁移零改动**：`git diff` 中 `docs/**` 与 `crates/screenpipe-db/src/migrations/**` 均为 0 文件。
2. **TaskKind 序列化串改名对既有用户 DB 的影响（需评审知悉）**：`knowledge_extract` 等新值经 `register_default_definitions` 的 upsert（`ON CONFLICT(definition_id) DO UPDATE`）写回 seed 行的 `kind` 列；运行时无按旧值 `'brain_extract'` 匹配 kind 列的查询，definition_id 保持 `brain.extract` 不变，因此任务 claim/激活路径不受影响；测试 4（含 task_migration）全绿佐证。残留风险：极旧镜像若存在未被 upsert 覆盖即被读取 kind 列的路径，`TaskKind::from_str` 会解析失败——当前代码路径未发现此形态。
3. **`brain-tools.ts` 扩展文件名**：桌面端会把扩展写入磁盘 ext 目录；已装用户磁盘上将残留旧 `brain-tools.ts` 文件。本批按计划改名（pi.rs 写入路径与工具名两端同步）；若 pi 加载器扫描目录内全部扩展，旧文件可能造成工具重复注册，建议批 2 或发布说明中提示清理。
4. **prompt/文案内的领域词**（agent 工具描述、system prompt、UI 标签）随改名等义替换（brain→knowledge），语义不变；与"不改提示词语义"约束不冲突，但模型可见文本确有字面变化，如实报备。
5. 未 commit、未 push、未使用 `git add -A`；`docs/archive/`、`docs/plans/knowledge-rename-execution-plan.md` 为会话开始前已存在的 untracked 内容，未触碰。

## 5. 未验证项

- `e2e`（wdio）specs：仅同步了字符串/命令名/文件名，未运行浏览器端到端（不在计划 §4 命令清单）。
- `bun run test` 的 24 个既有失败（§3.1）未修复——超出本批次授权（不顺手修 bug、不改断言语义），留待文案/测试批次。

## 6. 交付物核对

- [x] 验收 1：Rust 残留仅 §2.1–2.3 保护对象，无标识符残留
- [x] 验收 2：TS 残留仅 `brain-task`(8)、`brain.extract`(5)、`}brain-`(1)，均为持久化值/seed slug
- [x] 验收 3：`git diff --stat` 150 文件全部落在允许路径；docs/** 与 migrations/** 零改动；Cargo.lock 未动
- [x] 验收 4：§4 命令全绿（前端 24 个既有失败与改名无关，已用基线证据单独证明）
- [x] 验收 5：本报告含映射表、命令 exit_code 与通过数、剩余命中清单
- [x] 验收 6：零 schema 改动、+2694/−2694 对称、未顺手修 bug、未 commit/push
- 分析脚本留存：`.agent/tmp/rename_brain.py`（首轮批量替换，供复核）
