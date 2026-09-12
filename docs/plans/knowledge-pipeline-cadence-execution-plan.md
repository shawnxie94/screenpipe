---
id: plan-knowledge-pipeline-cadence
status: approved
owner: shawn
created: 2026-09-12
updated_at: 2026-09-12
plan_id: plan-knowledge-pipeline-cadence
plan_unit_id: root
base_commit: ee1f78dae
orchestration_mode: batch
execution_target: subagent
execution_backend: pi_subagent
runtime_adapter: zcode
logical_role: worker
subagent_role: zcode
subagent_scope: user
selected_subagent_model: builtin:bigmodel-coding-plan/GLM-5.3-Flash
thinking: low
fallback: false
parallel_mode: serial_same_worktree
acceptance_scope: batch
framework: docs/trd/personal-workbench-integration-framework.md#d-13
covers:
  - 沉淀管线正确性：失败终态不重投、input_hash 真实反映输入、错误消息可见、发现阶段预筛
  - ① 活动间隔切分优化：换内容锚点、相邻段合并、短段吸收（producer 升级 v2）
  - 四层节拍配置化（间隔重建 / 摘要 / WorkUnit / 知识），知识按粗粒度定时
  - ④ 知识投递三机制：模型提名 + 变更驱动 + 冷却期（不数数量、不全量）
  - 活动间隔重建独立成节拍，不再依赖 legacy 读取副作用；legacy 后端自动叙事下线
doc-covers: crates/screenpipe-engine/src/activity_ledger.rs, crates/screenpipe-engine/src/knowledge, crates/screenpipe-db/src/db/knowledge, apps/screenpipe-app-tauri/src-tauri/src/knowledge_runtime.rs, apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs
doc-verified: ee1f78dae
---

# 沉淀管线：切分与投递优化 + 正确性修复 + 四层节拍配置化

## 1. 背景（dev 真机冒烟证据）

`docs/reviews/evidence/dev-smoke-2026-09-12.md` 记录：3,994 间隔 → 92 摘要 → 113 WorkUnit → 1 知识；knowledge_jobs 1,583 条，失败 700+；同一 `activity-interval:13469` 1 小时被投 29 次；模型额度被耗尽（**根因是任务洪泛，不是模型能力**）。worker 为串行取任务（`worker.rs:243` 单 loop），不存在并发打满问题。

## 2. 目标与非目标

### 2.1 目标

1. **失败即终态**：失败任务不再被下一轮扫描重建；只有输入变化（input_hash 变化）才会重新投递。
2. **input_hash 真实反映输入**：抽取的三元组必须含参与的活动集合（现在传 `&[]`，导致同 scope 恒定哈希、幂等失效）。
3. **错误消息可见**：`knowledge_jobs` 增加错误消息列并写日志，用户可据此手动处理。
4. **发现阶段预筛**：无证据（`no_evidence`）等不成熟目标不投递，不产生无效模型调用。
5. **四层节拍全部可配置**（不再写死）：间隔重建、间隔摘要、WorkUnit 抽取、知识蒸馏。
6. **知识按粗粒度定时 + 三机制投递**（默认 1 天，可配 7 天）：**不做数量阈值**，也不全量投递，改用「模型提名 + 变更驱动 + 冷却期」——模型决定值不值得、输入变化决定有没有新料、冷却决定别重复烧钱。
7. **① 活动间隔切分优化**（确定性、可配）：换内容锚点（窗口标题降级为段内标签）、加相邻段合并规则、细段吸收，把 92% 的 <1 分钟碎片降到合理水平。详见 §4.4。
8. **间隔重建独立成节拍**：新增 app 侧 tick 调 `activity_ledger::reconcile_range`，不再依赖 legacy 生成路径「读台账顺带 refresh」的副作用。
9. **legacy 后端自动叙事下线**：`activity_history::start` 的自动生成分支停用（手动命令与物理代码保留）。

### 2.2 非目标（下一批）

| 项 | 批次 |
| --- | --- |
| 工作项身份（模型优先）+ 闭环判定（显式边界优先 + 30 分钟静默兜底） | 下一批（Plan B）；本批只做确定性的切分与合并 |
| WorkUnit 按工作项聚合、知识按「互不相同工作项」计数 | 下一批 |
| 前端活动视图文案 | 界面整理批 |

## 3. 现状锚点

| 事实 | 位置 |
| --- | --- |
| 去重只覆盖 `pending/running/paused` | `crates/screenpipe-db/src/db/knowledge/jobs.rs:66-69` |
| 抽取发现：逐个 `final` 间隔、scope=`app|标题`、`input_hash_for(&[], scope, skill)` | `crates/screenpipe-engine/src/knowledge/extract.rs:34-73` |
| 摘要发现：`missing-summary` 批 20 | `crates/screenpipe-engine/src/knowledge/summarize.rs:70-100` |
| 抽取成功即入队 compile（无定时） | `crates/screenpipe-engine/src/knowledge/extract.rs:275` |
| app 侧 5 分钟扫描（写死）驱动摘要 + 抽取 | `apps/screenpipe-app-tauri/src-tauri/src/knowledge_runtime.rs:443-450` |
| worker 串行 loop（单任务） | `crates/screenpipe-engine/src/knowledge/worker.rs:243-300` |
| 间隔重建入口（pub，供 tick 调用） | `crates/screenpipe-engine/src/activity_ledger.rs:48` `reconcile_range(db, start, end)` |
| legacy 自动叙事 30 秒 tick（同时是间隔重建的实际驱动） | `apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs:1893-1930` |
| 设置读写模式（`settings.extra` 自由键 + `SettingsStore::get`） | `activity_history.rs:1829-1834`（`setting_u64`）、`activity_history.rs:1885-1888`（写回） |
| 设置页现有字段 | `apps/screenpipe-app-tauri/components/settings/activities-settings.tsx`（启用开关 + 间隔下拉） |
| 最近迁移 | `20260911160000_knowledge_query_trace.sql` |

## 4. 交付物

### 4.1 失败终态与幂等

1. **去重语义**（`jobs.rs`）：`knowledge_enqueue_job` 的重复判定从 `state IN ('pending','running','paused')` 改为「同 `(kind, scope_key, input_hash)` 且 `state IN ('pending','running','paused','succeeded','failed')` 即视为已存在」——即终态（成功/失败）都拦重复投递；只有 **input_hash 变化**（输入真变了）才新建任务。语义注释写清楚：这正是"失败不重试、要人工处理"的落点。
2. **抽取 input_hash 含成员活动**（`extract.rs`）：scope 下的活动集合参与哈希（形如 `interval_id:start:end` 排序后拼接，参照 `summarize.rs::summary_input_hash` 的既有写法），使新增活动 → 新哈希 → 允许新任务。
3. **错误消息落库**（新迁移 `20260912100000_knowledge_job_error_message.sql`）：`knowledge_jobs` 加 `last_error_message TEXT`；worker 失败路径写入消息（当前只写 code），同时 `tracing::warn!` 带 `message`。不改已应用迁移。
4. **发现阶段预筛**（`extract.rs` / `summarize.rs`）：无证据引用、状态非 `final`、或未过静默 grace 的目标不投递；`no_evidence` 不再产生模型调用。
5. 遗留清理：把现有 `failed` 任务保留为终态（不批量重置），但**新增一条 SQL 维护路径**：`input_hash` 变化时旧失败记录不阻塞（由 1 的哈希语义天然满足）。

### 4.2 四层节拍配置化

设置键（写进 `settings.extra`，与既有 `activitiesIntervalMinutes` 同模式），默认值如下，**全部可配**：

| 键 | 含义 | 默认 | 允许范围 |
| --- | --- | --- | --- |
| `knowledgeReconcileMinutes` | ① 活动间隔重建（最近窗口） | 5 | 1–60 |
| `knowledgeReconcileWindowHours` | ① 重建回看窗口 | 2 | 1–48 |
| `knowledgeSummarizeMinutes` | ② 间隔摘要扫描 | 15 | 5–1440 |
| `knowledgeWorkUnitMinutes` | ③ WorkUnit 抽取扫描 | 60 | 5–1440 |
| `knowledgeDistillHours` | ④ 知识蒸馏周期 | 24 | 1–168 |
| `knowledgeDistillCooldownDays` | ④ 同流程重新蒸馏冷却期 | 7 | 1–90 |
| `activityMergeGapMinutes` | ① 相邻段合并阈值（§4.4） | 10 | 1–120 |
| `activityMinDwellSeconds` | ① 短段吸收阈值（§4.4） | 30 | 5–300 |

落地：

1. `knowledge_runtime.rs`：把写死的 `5 * 60` 改为读设置；新增 ① 的 tick（调用 `reconcile_range(now - window, now)`）与 ④ 的 tick（按 `knowledgeDistillHours`）。
2. **④ 投递改为三机制**（替代按数量投递）：移除「抽取成功即入队 compile」（`extract.rs:275`），由定时批次投递，单个 scope 满足以下三条才投：
   - **模型提名**：③ 抽取时同一模型调用额外输出「是否呈现可复用模式」与建议的流程归属（新增输出字段，不增加调用次数）；未被提名的 scope 不投。
   - **变更驱动**：比对该 scope 自上次蒸馏以来的 WorkUnit 集合 hash（复用 `input_hash` 机制，`knowledge_items.version` 旁记录 `last_distilled_input_hash`）；无变化则跳过。
   - **冷却期**：距上次蒸馏 < `knowledgeDistillCooldownDays` 则跳过。
   定时器只决定「什么时候看一眼」，不决定选谁，也不做数量阈值。
3. 设置页：在 `activities-settings.tsx` 增加上述字段（数字输入 + 单位 + 范围校验），旧 `activitiesIntervalMinutes` 保留为 legacy 展示并标注「仅影响旧叙事生成」。
4. 迁移兼容：旧配置存在时作为新键初值读取（不静默改动用户已有值）。

### 4.3 legacy 解耦与下线

1. `activity_history.rs`：`start` 的自动生成分支停用（保留 `generate` 命令与物理代码，供手动/回退）。停用后，间隔重建由 4.2 的 ① tick 独立负责——**必须先接上 ① 再停 legacy**，否则活动层断流。
2. 文档：`docs/reviews/evidence/dev-smoke-2026-09-12.md` 追加一节说明处置；roadmap 进行中条目同步。

### 4.4 ① 活动间隔切分优化（真实数据驱动）

**问题证据**（dev 库，3,994 段）：92% 的段 <1 分钟（平均 40 秒，每段仅 1–2 条证据）；`google chrome` **297 个身份 / 1,060 段**（每个网页标题算一个身份）；`wechat` 11 身份 / 373 段（「微信 (聊天)」210 段 + 「Using WeChat」72 + 「Window」40 + 联系人名各算身份）；`chatgpt` 8 身份 / **355 段**（每次切走再回来就新开一段）。系统/通知类噪音仅占 1%（44 段）——**碎的根因是身份太细 + 切了不合并，不是噪音**。

四项改动（全部确定性、无模型、参数可配）：

1. **换内容锚点**（`identity_for`）：浏览器身份改为「域名 + 路径首段」或用 `semantic_key`（现为整页标题）；桌面仍为 `document_path` > `semantic_key` > `app`；**窗口标题降级为段内标签**，不再参与身份键。
2. **加相邻段合并**：同一身份的两段若间隔 < `activityMergeGapMinutes`（默认 10）则合并成一段（"回到刚才那件事"）；同 `app` 内的弱锚点切换（标题变化）不切段，仅记录为段内切换。
3. **短段吸收**：段时长 < `activityMinDwellSeconds`（默认 30）且相邻存在更长的主段 → 吸收进主段；否则标记为 `short` 且不进摘要候选。
4. **producer 升级 `deterministic-v2`**：新旧切分语义可区分、可回退；旧段保留，新窗口按新规则重建。

验收观测（人工，写入报告）：重建后总段数、<1 分钟段占比、中位段时长、单位身份段数；目标为总段数降至 700–1,200、<1 分钟占比明显下降（预期 <20%）。

## 5. 测试与验收命令

| # | 命令 | 期望 |
| --- | --- | --- |
| 1 | `cargo test -p screenpipe-db --lib knowledge` | 全绿（含去重语义：failed/succeeded 拦截、哈希变化放行） |
| 2 | `cargo test -p screenpipe-engine --lib knowledge` | 全绿（含 input_hash 含成员活动、发现预筛、④ 三机制（提名/变更/冷却）、compile 不再即时入队） |
| 3 | `cargo test -p screenpipe-engine --lib activity_ledger` | 全绿（含新增：换锚点、相邻段合并、短段吸收、producer v2） |
| 4 | `cargo check -p screenpipe-core -p screenpipe-engine -p screenpipe-db` | exit 0 |
| 5 | `cd apps/screenpipe-app-tauri && bun run typecheck` | exit 0（设置页新增字段） |
| 6 | `cargo test -p screenpipe-db --test knowledge_correctness` | 全绿 |
| 7 | 静态核对：报告附「设置键 → 读取位置 → 默认值」表与「失败终态」语义说明 | 每项可定位 |
| 8 | 人工验收（协调者）：dev 端重启后观察 10 分钟——失败任务不再被重建；无 legacy 自动叙事日志；间隔按新节拍出现；执行 §4.4 的验收观测 | 观察记录写入报告 |

## 6. 风险与回退

| 风险 | 处置 |
| --- | --- |
| 去重收紧导致真实输入变化被拦 | 哈希必须包含成员集合（4.1.2），测试覆盖「新增活动 → 放行」 |
| 停 legacy 后间隔断流 | 严格顺序：先上 ① tick 并验证有间隔产出，再停 legacy；回退只需恢复 legacy 分支 |
| 切分语义变化影响存量段 | producer 升 v2 保留旧段；新规则只重建新窗口；极端不适时回退锚点规则（参数可配） |
| 模型提名引入新输出字段 | 字段可选，缺失即视为未提名（不阻断抽取）；提名不改调用次数 |
| 设置项过多 | 全部给默认值与范围校验，用户不改也能跑 |
| 现有 failed 存量任务 | 保持终态不动；报告给出它们的原因分布（错误消息落库后首次可见） |

**回退**：还原迁移（新列可留空）、恢复 legacy 分支、设置键回默认。

## 7. 交付边界

- 允许路径：`crates/screenpipe-db/src/migrations/20260912100000_knowledge_job_error_message.sql`、`crates/screenpipe-db/src/migrations/20260912101000_knowledge_distill_state.sql`（记录 `last_distilled_input_hash` 与 `last_distilled_at`）、`crates/screenpipe-db/src/db/knowledge/**`、`crates/screenpipe-engine/src/knowledge/**`、`crates/screenpipe-engine/src/activity_ledger.rs`、`crates/screenpipe-db/src/db/activity_ledger.rs`（如需同步身份/合并写入）、`apps/screenpipe-app-tauri/src-tauri/src/knowledge_runtime.rs`、`apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs`、`apps/screenpipe-app-tauri/components/settings/activity*-settings.tsx`、`docs/reviews/evidence/dev-smoke-2026-09-12.md`；报告限 `.agent/tmp/knowledge-pipeline-report.md` 与 `.agent/tmp/knowledge-pipeline/`。
- 禁止：改已应用迁移、`agent_skills.rs`、用户技能 store、知识校验规则（`registry/mod.rs` 的 SOP/DecisionRule 门槛不动，本批只改触发与幂等）、真实库写入、D-14 流程改造、工作项身份（下一批）。
- 本批不修改 `activity_interval_summaries` / `knowledge_work_units` 的表结构；切分优化只改 `identity_for`、合并与吸收规则及 producer 版本。
- 顺序硬约束：① 重建 tick 先落地并有产出 → 再停 legacy 自动叙事；④ 三机制先落地 → 再移除 `extract.rs:275` 的即时入队。
- 不 commit / 不 push；不得真调远程产品模型；不得清理用户现有 failed 任务。
- 单一 root、单一写者；子任务期间协调者不改 HEAD。
