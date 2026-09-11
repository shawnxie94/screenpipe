# 活动层 B02a 交付报告：间隔摘要生成（Summarize 生产侧）

日期：2026-09-11 ｜ 分支：zh-local ｜ 依据计划：`docs/plans/activity-summary-producer-execution-plan.md`（status: approved）

## 1. 结论

B02a 已实现并全部验收通过：新增 `Summarize` 任务种类，桌面 worker 在注入的模型通道上为「已结算（end_at ≤ now−5min）且缺摘要」的间隔生成自足、分档、带 1–3 条引证的中文摘要；同 `input_hash` 不重复调用模型，证据/提示词/模型任一变化即重算；三类越界（字数/关键字条数/引证）严格化重试一次，仍不合格以 `summary_invalid` 永久失败且不落库。真实库副本（/tmp/b01-compare.sqlite，3779 间隔）上 discovery 一轮入队 20 个 job、20/20 摘要成功落库、二次运行 0 次新模型调用。

### 实现过程中确认的两个关键事实（影响方案）

1. **`knowledge_jobs.kind` 有 CHECK 约束**（create_brain 迁移固化为 `('extract','compile','answer','backfill_extract','office_sync','history_migration','cleanup')`），新种类 `summarize` 直接入库被拒。SQLite 无法原地改 CHECK，故新增第二个迁移 `20260911150000_knowledge_jobs_allow_summarize.sql` 单事务重建该表（保留全部行/列/三个索引，含 partial unique `idx_knowledge_jobs_active_input`）。
2. **frame 证据文本在 `frames.full_text/accessibility_text`**：迁移 `20260613130000_unify_ocr_text_into_frames` 已把 `ocr_text` 统一进 frames，真实副本中 `ocr_text` 表不存在（27/27 frame 证据靠 full_text 命中）。摘要的证据摘录按 unified frames 列读取（与 B01 观察加载器一致）。`extract.rs` 仍读 `ocr_text` 属历史遗留，本批禁改，留给后续批次。

## 2. 修改/新增的文件

### 新增
| 文件 | 内容 |
| --- | --- |
| `crates/screenpipe-db/src/migrations/20260911140000_activity_summary_evidence_refs.sql` | `activity_interval_summaries` 加 `evidence_refs TEXT NOT NULL DEFAULT '[]'` + `idx_activity_summaries_input(input_hash)`（计划 §3，单事务） |
| `crates/screenpipe-db/src/migrations/20260911150000_knowledge_jobs_allow_summarize.sql` | 重建 `knowledge_jobs`，kind CHECK 加入 `'summarize'`（计划外但必要，见上） |
| `crates/screenpipe-engine/src/knowledge/summarize.rs` | B02a 核心：`discover_and_enqueue`（单轮上限 20、settled 5 分钟宽限、active-input 去重）、`summarize_handler`/`run_summarize`/`summarize_interval`（有界证据包 24k、input_hash、幂等跳过、一次严格化修复重试、校验、落库）、14 个单测 |
| `crates/screenpipe-engine/tests/activity_summary_real_db.rs` | 真实库副本集成测试（`SCREENPIPE_ACTIVITY_DB` 驱动，默认跳过；TempDir 拷贝、假执行体、字数落档/引证/幂等断言） |

### 修改（全部在允许路径；`*` 为连带编译适配）
| 文件 | 内容 |
| --- | --- |
| `crates/screenpipe-db/src/db/knowledge/types.rs` | `KnowledgeJobKind::Summarize`（as_str `summarize`、from_str、优先级 8 高于 Extract 的 10）；roundtrip/优先级断言 |
| `crates/screenpipe-db/src/db/knowledge/jobs.rs` | `task_binding(("knowledge.summarize","extract"))`、definition kind 串 `knowledge_summarize` |
| `crates/screenpipe-db/src/db/activity_ledger.rs` | `activity_summary_upsert`（引证 1–3 且必须存在于该间隔 `activity_evidence`，否则拒；同 hash 幂等返回；异 hash 替换）、`activity_summary_by_input_hash`（幂等读/待生成判定）、`activity_intervals_missing_summary` 扩展 `settled_before: Option<DateTime>` 参数（None 保持 B01 行为）；+4 个 activity 单测 |
| `crates/screenpipe-db/src/db/mod.rs`、`src/lib.rs` | 导出 `ActivitySummaryRow`、`ActivitySummaryEvidenceRef` |
| `crates/screenpipe-engine/src/knowledge/prompts/mod.rs` | `SUMMARY_SYSTEM`/`SUMMARY_USER`/`SUMMARY_REPAIR_INSTRUCTION`/`SUMMARY_PROMPT_VERSION`（zh-summary-v1）/`SUMMARY_PRODUCER`；分档字数、5–12 关键字含专名、1–3 引证、反例（禁「处理了 X 工作」式空话） |
| `crates/screenpipe-engine/src/knowledge/executor.rs` | `BudgetedExecutor::identity()`（input_hash 的模型身份来源） |
| `crates/screenpipe-engine/src/knowledge/worker.rs` | `definition_for_kind`（`knowledge.summarize`）、`kind_for_definition`、public loop definitions 加 Summarize |
| `crates/screenpipe-engine/src/knowledge/mod.rs` | `pub mod summarize` |
| `apps/screenpipe-app-tauri/src-tauri/src/knowledge_runtime.rs` | 5 分钟 ticker 内先摘要 discovery 后提取 discovery（D-15）；注册 `Summarize` handler |
| `crates/screenpipe-engine/src/routes/activity_ledger.rs` * | REST missing-summary 端点调用点补 `None` 参数（保持原行为），签名扩展的连带适配；该文件不在允许路径清单内，属编译必需的最小改动，请协调会话知悉 |

## 3. 验收命令与结果（§6，全部实跑）

| # | 命令 | 结果 | exit |
| --- | --- | --- | --- |
| 1 | `cargo test -p screenpipe-engine --lib summarize` | **14 passed**（分档选择、字数/关键字/引证越界各修复一次、修复后仍越界即 `summary_invalid` 不落库、同 hash 零调用、证据变化重算、模型身份变化重算、预算上限、无证据零调用、discovery 结算窗与去重、frame unified 文本） | 0 |
| 2 | `cargo test -p screenpipe-db --lib activity` | **17 passed**（含新增：引证越界/伪造拒绝、upsert 幂等、按 hash 查待生成、settled_before 过滤） | 0 |
| 3 | `SCREENPIPE_LEGACY_DB=/tmp/knowledge-rename-backup-2026-09-10/zhiji-dev-db.sqlite cargo test -p screenpipe-db --test legacy_db_upgrade` | **1 passed**（真实库升级路径创建新列/新索引） | 0 |
| 4 | `SCREENPIPE_ACTIVITY_DB=/tmp/b01-compare.sqlite cargo test -p screenpipe-engine --test activity_summary_real_db` | **1 passed**；输出：`intervals missing a summary: 3779` → `discovery round created 20 summarize jobs` → `summarized 20 intervals, 0 failed, 20 model calls`（假执行体；断言写入行数=成功数、二次运行 0 次新调用、band/字数落档、引证 1–3 且指向保留证据、已摘要间隔离开 backlog） | 0 |
| 5 | `cargo test -p screenpipe-db --test knowledge_correctness` / `cargo test -p screenpipe-engine --test knowledge_correctness` | **8 passed** / **13 passed**（提取路径未改） | 0 |
| 6 | `cargo check -p screenpipe-db -p screenpipe-engine` | Finished（仅既有 warning） | 0 |
| 7 | `cd apps/screenpipe-app-tauri && bun run test:tauri knowledge_runtime` | **1 passed**（走 native 构建队列，2m58s 释放；`gen/schemas` 无残留改动） | 0 |

补充回归：`cargo test -p screenpipe-engine --lib knowledge::worker`（3 passed）、`cargo test -p screenpipe-db --lib knowledge`（20 passed）。

## 4. 未做 / 未验证项

- **未真调远程模型**（约束）：所有测试走注入的假执行体；桌面真实通道（Pi sidecar）只做了编译与单测级验证，未在本机起 app 实跑。
- **Discovery 无毒丸隔离**：完全无可用文本的间隔（真实副本 7/3779）无法生成摘要，会每轮 discovery 重新入队并以 `no_evidence` 永久失败。规模小不阻塞进度（oldest-first + 上限 20，7 个 < 20），但每 5 分钟会产生约 7 个失败 job（终端 job 有 prune 兜底）。建议 B02b 在 discovery 查询加「存在可读文本证据」过滤或失败冷却。
- **提取路径（extract.rs）仍读 `ocr_text`**：unify 迁移后该表已不存在，WorkUnit 抽取对 frame 证据实际取不到文本。本批明令禁改，仅在此报告，建议后续批次跟进。
- 计划未要求 UI/读源切换（B02b）、WorkUnit 取数扩展（B03），均未动。
- `TaskKind::ActivitySummary`（engine tasks 模块既有占位）与本批 `KnowledgeJobKind::Summarize` 的映射未接（计划未要求）。

## 5. 风险

| 风险 | 现状与缓解 |
| --- | --- |
| 迁移 `20260911150000` 重建 `knowledge_jobs` | 单事务、行/索引全保留；legacy_db_upgrade 在 57MB 真实备份上验证；表行数受 prune 约束（≤1000 终态），重建耗时毫秒级 |
| 模型输出持续越界 → 反复重试 | 每 job 预算 ≤3（首次+修复=2 次调用即止）；job `summary_invalid` 永久失败；discovery 每 5 分钟可能为新 job 重试（见上毒丸项，量级小） |
| 预设切换/证据重建触发大量重算 | 属预期行为（input_hash 变化即重算）；单轮上限 20 + 时间窗控制速率 |
| 一次性测试写真实副本 | 集成测试先 APFS 拷贝到 TempDir（panic 亦清理），只读原始副本；已清理一次失败运行泄漏的 2.9G 拷贝 |

## 6. 备注

- 未执行任何 git commit/add/push；未改 docs/**、既有迁移、前端 lib/components、activity_history.rs、extract.rs 取数逻辑。
- 测试产生的临时文件均已清理（$TMPDIR 下仅剩此前会话的 legacy-upgrade 拷贝与 372B quarantine reserve 标记，未动）。
