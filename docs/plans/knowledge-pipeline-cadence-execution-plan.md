---
id: plan-knowledge-pipeline-cadence
status: approved
owner: shawn
created: 2026-09-12
updated_at: 2026-09-12
plan_id: plan-knowledge-pipeline-cadence
plan_unit_id: root
base_commit: 769c80189
orchestration_mode: batch
execution_target: subagent
execution_backend: pi_subagent
runtime_adapter: pi
logical_role: worker
subagent_role: worker
subagent_scope: user
selected_subagent_model: openai-codex/gpt-5.6-luna
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

`docs/reviews/evidence/dev-smoke-2026-09-12.md` 的 dev 库副本记录：3,994 间隔、92 摘要、113 WorkUnit、1 知识，knowledge_jobs 1,583 条；这些是存量计数，不是漏斗转化率。用户确认额度耗尽与重复任务有关，不能归因模型能力。worker 为串行取任务。此前「一小时投 29 次」的时间筛选格式未经统一验证，撤回该速率判断。

2026-09-12 首轮实现未通过独立复核，证据 `.agent/tmp/knowledge-pipeline-r1-review/feedback.md`。用户明确批准返工：关闭自动重试；合并保持证据时间归属；旧段保留追溯但重建窗口只启用一个版本。本次修订只恢复 Round 1（§4.1/§4.4）开工资格，Round 2（§4.2/§4.3）不得冒充已实现。

## 2. 目标与非目标

### 2.1 目标

1. **失败即终态**：内部失败/超时不自动重试，public task 与 knowledge job 同步终态；同输入不被发现扫描重建。只有真实输入变化才是新任务，原任务仅用户手动重试可重新打开。
2. **input_hash 真实反映输入**：抽取的三元组必须含参与的活动集合（现在传 `&[]`，导致同 scope 恒定哈希、幂等失效）。
3. **错误消息可见**：`knowledge_jobs` 增加错误消息列并写日志，用户可据此手动处理。
4. **发现阶段预筛**：无证据（`no_evidence`）等不成熟目标不投递，不产生无效模型调用。
5. **四层节拍 + 发现回看窗口 + 调用预算全部可配置**（不再写死）：间隔重建、间隔摘要、WorkUnit 抽取、知识蒸馏各自独立的节拍，**以及各阶段自己的发现回看窗口**（现为 ②③ 共用写死 26h）、每轮投递上限、单次调用/单任务超时与每任务调用上限。证据（2026-09-12）：②③ 的 26h 回看与 5 分钟 tick 都不可配，导致早于 26h 的 418 个未摘要间隔永远不会被处理，用户既不能拉长窗口补历史也不能缩短窗口控制消耗。
6. **知识按粗粒度定时 + 三机制投递**（默认 1 天，可配 7 天）：**不做数量阈值**，也不全量投递，改用「模型提名 + 变更驱动 + 冷却期」——模型决定值不值得、输入变化决定有没有新料、冷却决定别重复烧钱。
7. **① 活动间隔切分优化**（确定性、可配）：换内容锚点（窗口标题降级为段内标签）、加相邻段合并规则、细段吸收。**2026-09-12 真机副本实测已证伪原目标**：真实窗口 112/112 个相邻边界均为跨身份切换（semantic 68 / site 40 / document_path 0），"把 <1 分钟碎片降到合理水平"不能由锚点与合并规则达成，须改由工作项层（Plan B）解决；本项保留锚点稳定性、证据不变量与单活跃版本三项已实现能力。详见 §4.4。
8. **间隔重建独立成节拍**：新增 app 侧 tick 调 `activity_ledger::reconcile_range`，不再依赖 legacy 生成路径「读台账顺带 refresh」的副作用。
9. **legacy 后端自动叙事下线**：`activity_history::start` 的自动生成分支停用（手动命令与物理代码保留）。

### 2.2 非目标（下一批）

| 项 | 批次 |
| --- | --- |
| 工作项身份（模型优先）+ 闭环判定（显式边界优先 + 30 分钟静默兜底） | 下一批（Plan B）；本批只做确定性的切分与合并 |
| **碎片目标**（原 §2.1 #7 的"降低 <1 分钟碎片占比"） | **移交 Plan B**：2026-09-12 真机副本实测 112/112 相邻边界均为跨身份切换（semantic 68 / site 40 / document_path 0），锚点与合并规则不可达，须由工作项层解决 |
| 桌面 UI 展示知识任务错误消息（R1 已透出 DTO/API，未进界面） | **R2**（与 §4.2 设置页同批） |
| v1 存量证据时间越界（34 条实质越界 + 35 条 end 边界口径） | **独立小批**（不在 R2）；见 §2.3 |
| WorkUnit 按工作项聚合、知识按「互不相同工作项」计数 | 下一批 |
| 前端活动视图文案 | 界面整理批 |

### 2.3 后续批次登记（不在 R1/R2 范围，已指定归属）

| 项 | 批次 | 证据 | 备注 |
| --- | --- | --- | --- |
| v1 存量证据时间越界 | 独立小批 | 真库副本统计：9,423 条证据中 69 条越界——**35 条恰落在 `end` 边界**（`[start,end)` 口径问题）、**34 条实质早于 `start` 超过 60s**（疑与音频身份继承 / 未观测切分有关）；R1 未引入亦未修 | 修复前先判定：边界口径类走"改语义或改断言"，实质类需定位成因；不得改已应用迁移，不得清洗真库现有行 |
| 碎片/工作项聚合 | Plan B | 同 §2.2 实测结论 | 本计划不再承担该目标 |
| 桌面 UI 错误可见 | R2 | R1 仅验证到 `JobDto.last_error_message` 透出 | 与设置页同批交付 |
| 重复模型调用实际发生量审计 | 待定（Plan B 或独立小批） | 归因轮快照口径为 0 重复；未运行 worker 级审计 | 幂等口径修复后可闭合 |

## 3. 现状锚点

| 事实 | 位置 |
| --- | --- |
| 去重只覆盖 `pending/running/paused` | `crates/screenpipe-db/src/db/knowledge/jobs.rs:66-69` |
| 抽取发现：逐个 `final` 间隔、scope=`app|标题`、`input_hash_for(&[], scope, skill)` | `crates/screenpipe-engine/src/knowledge/extract.rs:34-73` |
| 摘要发现：`missing-summary` 批 20 | `crates/screenpipe-engine/src/knowledge/summarize.rs:70-100` |
| 抽取成功即入队 compile（无定时） | `crates/screenpipe-engine/src/knowledge/extract.rs:275` |
| app 侧 5 分钟扫描（写死）驱动摘要 + 抽取 | `apps/screenpipe-app-tauri/src-tauri/src/knowledge_runtime.rs:443-450` |
| 发现回看窗口写死 26h、批 20、调用预算写死（45s/120s/3 次） | `knowledge_runtime.rs:451`、`summarize.rs:35`、`executor.rs:66-70` |
| worker 串行 loop（单任务） | `crates/screenpipe-engine/src/knowledge/worker.rs:243-300` |
| 间隔重建入口（pub，供 tick 调用） | `crates/screenpipe-engine/src/activity_ledger.rs:48` `reconcile_range(db, start, end)` |
| legacy 自动叙事 30 秒 tick（同时是间隔重建的实际驱动） | `apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs:1893-1930` |
| 设置读写模式（`settings.extra` 自由键 + `SettingsStore::get`） | `activity_history.rs:1829-1834`（`setting_u64`）、`activity_history.rs:1885-1888`（写回） |
| 设置页现有字段 | `apps/screenpipe-app-tauri/components/settings/activities-settings.tsx`（启用开关 + 间隔下拉） |
| 最近迁移 | `20260911160000_knowledge_query_trace.sql` |

## 4. 交付物

### 4.1 失败终态与幂等

1. **失败策略与去重必须同时落地**：
   - `knowledge_fail_job` 对 transient、permanent、模型错误、step timeout 均一次失败即 `failed`，不再设回 `pending/queued`。事务内同步 public run、attempt、event，保留 code/message。不是只改 max_attempts 默认值。
   - knowledge owner 的 lease 回收不得自动再次执行模型：失联执行标为可人工处置的终态，旧 worker 返回不得写结果。保留心跳、token 围栏；不得改变非 knowledge owner 的重试策略。
   - `knowledge_retry_job` 与 public 手动 retry 为显式用户通道；必须测试重开一次、并发手动重试不重复投递、历史错误仍可追溯。worker timeout 日志不再声称自动重试。
   - 同 `(kind, scope_key, input_hash)` 的 pending/running/paused/succeeded/failed 拦重复投递。物理索引若采用 `(kind,input_hash)`，须证明所有自动路径哈希含 scope，并覆盖不同 scope 测试，不能只靠注释保证等价。
   - 审核终态清理、cancelled 及 public lease recovery 对去重的影响；同输入失败不可因后台清理而自动复活（必要时保留轻量去重记录或保留自动任务终态）。不清理用户现有 failed 记录。
2. **抽取 input_hash 含成员活动**（`extract.rs`）：scope 下的活动集合参与哈希（形如 `interval_id:start:end` 排序后拼接，参照 `summarize.rs::summary_input_hash` 的既有写法），使新增活动 → 新哈希 → 允许新任务。
3. **错误消息落库**（新迁移 `20260912100000_knowledge_job_error_message.sql`）：`knowledge_jobs` 加 `last_error_message TEXT`；worker 失败路径写入消息（当前只写 code），同时 `tracing::warn!` 带 `message`。不改已应用迁移。
4. **发现阶段预筛**（`extract.rs` / `summarize.rs`）：无证据引用、状态非 `final`、或未过静默 grace 的目标不投递；`no_evidence` 不再产生模型调用。
5. 发现预筛在 LIMIT 前完成，避免无证据/非 final/非活跃旧版本永久占满候选页。执行时再次确认活动版本有效；排队后被新版本替代的任务不再调用模型。R1 将错误透出 DTO/API；实际桌面错误展示与手动入口在 R2 验证，不把日志/API 透出宣称为 UI 已验收。旧行 NULL 错误不能事后恢复。

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
| `knowledgeDiscoveryLookbackHours` | ②③ 发现回看窗口（现写死 26h） | 26 | 1–168 |
| `knowledgeDiscoveryBatch` | ②③ 每轮发现投递上限（现写死 20） | 20 | 1–500 |
| `knowledgeCallTimeoutSeconds` | 单次模型调用超时（现写死 45s） | 45 | 10–600 |
| `knowledgeStepTimeoutSeconds` | 单任务总时长（现写死 120s） | 120 | 30–1800 |
| `knowledgeMaxModelCallsPerJob` | 每任务模型调用上限（现写死 3） | 3 | 1–10 |

落地：

1. `knowledge_runtime.rs`：把写死的 `5 * 60` 改为读设置；新增 ① 的 tick（调用 `reconcile_range(now - window, now)`）与 ④ 的 tick（按 `knowledgeDistillHours`）。
2. **④ 投递改为三机制**（替代按数量投递）：移除「抽取成功即入队 compile」（`extract.rs:275`），由定时批次投递，单个 scope 满足以下三条才投：
   - **模型提名**：③ 抽取时同一模型调用额外输出「是否呈现可复用模式」与建议的流程归属（新增输出字段，不增加调用次数）；未被提名的 scope 不投。
   - **变更驱动**：比对该 scope 自上次蒸馏以来的 WorkUnit 集合 hash（复用 `input_hash` 机制，`knowledge_items.version` 旁记录 `last_distilled_input_hash`）；无变化则跳过。
   - **冷却期**：距上次蒸馏 < `knowledgeDistillCooldownDays` 则跳过。
   定时器只决定「什么时候看一眼」，不决定选谁，也不做数量阈值。
3. 设置页：在 `activities-settings.tsx` 增加上述字段（数字输入 + 单位 + 范围校验），旧 `activitiesIntervalMinutes` 保留为 legacy 展示并标注「仅影响旧叙事生成」。
4. 迁移兼容：旧配置存在时作为新键初值读取（不静默改动用户已有值）。
5. 桌面 UI：知识任务失败时展示 `last_error_message`（R1 已透出到 `JobDto`/API，本轮接到界面并给出手动重试入口）。

### 4.3 legacy 解耦与下线

1. `activity_history.rs`：`start` 的自动生成分支停用（保留 `generate` 命令与物理代码，供手动/回退）。停用后，间隔重建由 4.2 的 ① tick 独立负责——**必须先接上 ① 再停 legacy**，否则活动层断流。
2. 文档：`docs/reviews/evidence/dev-smoke-2026-09-12.md` 追加一节说明处置；roadmap 进行中条目同步。

### 4.4 ① 活动间隔切分优化（真实数据驱动）

**问题证据**（dev 库副本）：3,994 段中约 92% <1 分钟；final 平均约 40 秒、provisional 约 24 秒，不是中位数；1–2 条证据的段占多数。Chrome 297 身份/1,060 段、WeChat 11/373、ChatGPT 8/355。枚举的系统应用名称命中 44 段，不代表全部噪音仅 1%。身份变化即时切段是已确认机制，但摘要质量与切分之间的完整因果尚未验证。

四项改动（确定性、无模型、参数可配；不得以段数下降替代正确性）：

1. **稳定内容锚点**：明确文档路径/语义对象优先于低精度网页分组；浏览器缺实体标识时才用 host+路径首段，窗口标题只作标签。不同明确文档、对话实体不可因同 app/域名被误并。无法识别对象时记录低置信兜底，不声称已经识别工作项。
2. **合并资格**：仅时间相邻、同对象或可证实的弱标题变化可合并，`activityMergeGapMinutes` 是上限，不是同 app 即同任务的充分条件。A→B→A 中 B 是独立对象时三段保留；不跨明确会议/未观测边界吸收。无观测时长不得直接充当连续工作时长。
3. **短段与证据不变量**：时长 < `activityMinDwellSeconds` 仅是候选条件。合并需保持对象归属，重新确定包含所有原证据/动作的时间范围，并保证不跨第三方段；每条证据时间属于持有它的间隔、源引用不丢不重复。最终合并段重新执行 retention 上限与记账，不能简单串接已截断证据或突破 B01 限额；会议豁免仍正确。不能证明安全时保留短段，允许查询派生 short 标志，不增造不合法 state；短且关键的事实不因时长被删除。
4. **单活跃版本 + 历史追溯**：producer 可升 `deterministic-v2`，但必须增加明确的活跃选择规则；仅改常量不是迁移或回退方案。
   - 一次重建覆盖窗口内只允许一个确定性版本参与默认列表、统计、摘要、WorkUnit 输入；旧段及其 evidence/actions/summary/既有引用保留，用历史 ID 查询仍可追溯。
   - 新结果写入与活跃切换在同一事务中提交，失败保留旧版本；重跑幂等。空窗口也有已重建覆盖状态，不能回落到旧版本重复生成。
   - 处理旧段跨越重建窗口边缘的情况：可扩大重建到完整边界或采用显式覆盖片段映射，但不能整段隐藏后丢失窗口外数据；不得靠新旧两套并排返回解决边缘。
   - 所有生产/展示读入口复用统一活跃选择；历史详情读取保持按 ID 可用。排队后过期版本在执行前复检；回退显式切换已保留覆盖范围，不把版本字符串字典序当新旧关系。
   - 新迁移承载活跃元数据/覆盖记录；不改已应用迁移、不手工操作真库。迁移后的旧段默认可见，只有成功重建覆盖部分才切换。

验收（两层）：
- **合成层**：内存库/合成 fixture 必须证明失败不重试、A→B→A 不误并、证据时间范围、保留上限、v1→v2 同窗不重复、边缘不丢失、失败原子性、重复重建幂等、历史引用可查及显式回退。
- **真实副本层**（2026-09-12 新增，因合成层放过了两处真实缺陷）：在 dev 库副本上对 **v2 产出**独立断言区间不重叠、每条证据 `occurred_at` 落在所属区间内、无孤儿证据、**同窗重复 reconcile 的段集合稳定（`interval_key` 集合不变）且发现输入指纹（`discovery_input_hash`）不变、该窗口零新增投递**（2026-09-12 裁决：行 id 是内部产物、保留策略会按设计删除空段，验收以用户可感知的"同内容不重复消耗模型额度"为准；发现哈希成员改用稳定的 `interval_key`而非行 id）、被覆盖 v1 仍可按 id 查、覆盖范围单活跃版本；v1 存量越界单独统计，不并入 v2 判定。副本分布只作观测，不设未经实测的 700–1,200 段或 <20% 门槛；本轮不启动真机或调用远程产品模型。

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
| 9 | `SP_REAL_DB_COPY=<副本> cargo test -p screenpipe-engine --test real_db_resegmentation -- --ignored --nocapture` | exit 0；v2 产出不变量全过（含段集合/输入指纹稳定与证据时间归属）。v1 存量越界单独统计 |
| 8 | 人工验收（协调者）：dev 端重启后观察 10 分钟——失败任务不再被重建；无 legacy 自动叙事日志；间隔按新节拍出现；执行 §4.4 的验收观测 | 观察记录写入报告 |

## 6. 风险与回退

| 风险 | 处置 |
| --- | --- |
| 去重收紧导致真实输入变化被拦 | 哈希必须包含成员集合（4.1.2），测试覆盖「新增活动 → 放行」 |
| 停 legacy 后间隔断流 | 严格顺序：先上 ① tick 并验证有间隔产出，再停 legacy；回退只需恢复 legacy 分支 |
| 切分语义变化影响存量段 | 历史留档与活跃选择分离，切换原子化并测试窗口边缘；回退走显式版本激活，不只改 producer 常量 |
| 模型提名引入新输出字段 | 字段可选，缺失即视为未提名（不阻断抽取）；提名不改调用次数 |
| 设置项过多 | 全部给默认值与范围校验，用户不改也能跑 |
| 现有 failed 存量任务 | 保持终态不动；新列只保证新增失败的消息，无法恢复历史丢失的错误正文 |

**回退**：不逆改已应用迁移；保留兼容新列，以显式活跃版本切换恢复历史覆盖。legacy 回退必须关新自动 tick，避免双驱动；参数改回原值不能替代数据版本回退。

## 7. 交付边界

- 允许路径：`crates/screenpipe-db/src/migrations/20260912100000_knowledge_job_error_message.sql`、`crates/screenpipe-db/src/migrations/20260912101000_knowledge_distill_state.sql`（记录 `last_distilled_input_hash` 与 `last_distilled_at`）、`crates/screenpipe-db/src/db/knowledge/**`、`crates/screenpipe-engine/src/knowledge/**`、`crates/screenpipe-engine/src/activity_ledger.rs`、`crates/screenpipe-db/src/db/activity_ledger.rs`（如需同步身份/合并写入）、`apps/screenpipe-app-tauri/src-tauri/src/knowledge_runtime.rs`、`apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs`、`apps/screenpipe-app-tauri/components/settings/activity*-settings.tsx`、`docs/reviews/evidence/dev-smoke-2026-09-12.md`；报告限 `.agent/tmp/knowledge-pipeline-report.md` 与 `.agent/tmp/knowledge-pipeline/`。
- 禁止：改已应用迁移、`agent_skills.rs`、用户技能 store、知识校验规则（`registry/mod.rs` 的 SOP/DecisionRule 门槛不动，本批只改触发与幂等）、真实库写入、D-14 流程改造、工作项身份（下一批）。
- R1 返工补充允许路径：`crates/screenpipe-db/src/migrations/20260912102000_activity_ledger_active_versions.sql`、`crates/screenpipe-db/src/db/tasks/**`、`crates/screenpipe-engine/src/routes/activity_ledger.rs`、`crates/screenpipe-db/tests/knowledge_correctness.rs`；必要类型/re-export 限 `crates/screenpipe-db/src/db/mod.rs` 与 `crates/screenpipe-engine/src/lib.rs`，不可顺带改无关功能。
- 不修改 `activity_interval_summaries` / `knowledge_work_units` 的既有表结构；允许新增活跃元数据表/视图及必要迁移以满足 §4.4，不再以迁移配额拒绝版本隔离。R1 报告及验收产物限 `.agent/tmp/knowledge-pipeline-r1*`；计划/readiness/roadmap 由协调者维护。
- R1 增加验收：`cargo test -p screenpipe-db --lib activity_ledger`、`cargo test -p screenpipe-db --lib tasks`、`cargo test -p screenpipe-engine --lib routes::activity_ledger`。R2 原生测试只通过 `cd apps/screenpipe-app-tauri && bun run test:tauri`；真实模型/桌面验收另获用户许可，不以十分钟观察替代日/周调度的虚拟时钟测试。
- 顺序硬约束：① 重建 tick 先落地并有产出 → 再停 legacy 自动叙事；④ 三机制先落地 → 再移除 `extract.rs:275` 的即时入队。
- 不 commit / 不 push；不得真调远程产品模型；不得清理用户现有 failed 任务。
- 单一 root、单一写者；子任务期间协调者不改 HEAD。
