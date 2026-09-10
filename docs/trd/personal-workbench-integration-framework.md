---
id: screenpipe-personal-workbench-integration-framework
type: trd
scope: zh-local-personal-workbench
version: "1.3"
status: draft
direction_status: confirmed
implementation_status: not_started
created_at: 2026-09-10
updated_at: 2026-09-10
sources:
  - docs/roadmap.md
  - docs/prd/personal-brain-local-first.md
  - docs/trd/personal-brain-local-first.md
  - docs/plans/personal-brain-correctness-execution-plan.md
  - docs/reviews/personal-workbench-integration-framework-readiness.yaml
  - docs/archive/zhiji/README.md
  - docs/archive/zhiji/trd/product-integration-framework.md
related:
  - docs/roadmap.md
  - docs/prd/personal-brain-local-first.md
  - docs/trd/personal-brain-local-first.md
  - docs/reviews/personal-workbench-integration-framework-readiness.yaml
---

# screenpipe zh-local 个人工作台整合框架

<!-- doc-covers: crates/screenpipe-core/src/tasks, crates/screenpipe-db/src/db/brain, crates/screenpipe-db/src/migrations, crates/screenpipe-engine/src/brain, crates/screenpipe-engine/src/tasks, crates/screenpipe-connect/src/connections, crates/screenpipe-connect/src/office -->
<!-- doc-verified: 18b2379be -->

本框架替代原 `zhiji` 独立 MIT 拆分路线。当前目标是直接在活跃的 `screenpipe` `zh-local` 分支上满足个人日常需求，并持续吸收上游有效优化；不再为不明确的商业化前景维护一套难以局部拆分、难以追赶上游的独立产品。

职责边界：本文件只维护跨阶段职责、Core 公共语义、设计决策和契约冻结顺序；**阶段顺序、优先级、进度和验收以 [roadmap](../roadmap.md) 与 `docs/plans/` 为准，本文件不复述**。

## 1. 总体决策

### D-01：继续在 zh-local 上迭代

`screenpipe zh-local` 是唯一持续开发的产品和事实中枢。原 `zhiji` 代码、独立身份、独立数据库和 MIT 拆分计划停止实施并归档；其中的设计、验证和取舍材料保留为历史输入。

### D-02：通用内核优先，但不提前建设插件平台

screenpipe 逐步形成“稳定 Core + 内置 Domain Pack + Connector/Publication Adapter”的结构。Core 只承载跨领域不变量，不根据 Infinitum、Lumina、zhihuan 或 Personal OS 编写特殊分支。首期只支持随应用交付的内置扩展，不做动态第三方插件安装和生态治理。

### D-03：zhihuan 的平台设计并入 screenpipe，研究语义单独成包

吸收 zhihuan 的 Workspace、Connector、Workflow、Run/Attempt、Artifact、质量门、审批、审计、恢复和 Agent Engine Adapter 设计，作为 screenpipe 的平台演进输入。持续研究、研究任务、Claim 和研究产物属于 `research-pack`，不恢复第二套 Zhihuan Core。

### D-04：Personal OS 保持文件真源

Personal OS 通过 `personal-pack` 和 File Ledger Adapter 接入。Goal、Quest、Review 仍由 Personal OS 文件协议和 kb 入口维护；screenpipe 只保存跨域关系、投影和运行引用，不移动 Developer 目录、不复制 work/memory/agents 正文、不接管 kb 运维。

### D-05：Infinitum 只做雷达预处理，复杂综合归 screenpipe

Infinitum 保留 RSS/Atom 抓取、正文补全、清洗、去重、质量判断和基础事件聚合。目标相关性、跨周期比较、证据化简报、用户反馈和行动建议在 screenpipe 的 Workflow 中运行。原 Infinitum 日报作为回归样本，不再继续扩展为独立综合真源。

### D-06：Lumina 新采集直入 screenpipe，发布单向投影

Lumina 浏览器扩展成为 screenpipe 的采集客户端，新网页不再先进入 Lumina 后台。screenpipe 生成并审核的知识或 Artifact，按需通过 Publication Adapter 单向投影到 Lumina 的公众文章、专栏或 RSS 展示面；不做双向业务同步。

### D-07：扩展点只有一条注册路径，Core 保持闭集

内置包新增对象类型、来源种类或任务种类，必须走唯一的“**包注册清单 + 迁移脚本 + `task_owner_state` 初始化**”路径：在同一处声明类型/种类、资源类别、优先级、重试策略、处理器注册和兼容迁移。Core 的 `TaskKind`、`TaskResourceClass`、`SourceKind` 保持编译期闭集，不建动态类型注册表，不为未知种类提供兜底路径；`task_definitions.kind` 为 TEXT，新增种类必须同时补齐前后端契约与注册清单。

本决定接受“新增内置包必然修改 Core 枚举与迁移”这一事实，把它约束成可枚举、可 review 的机械改动；被禁止的是**包内自建调度循环**，以及绕过 Core 直接写正式状态。

### D-08：Connector 就是现有连接层，办公不单独成包

所有外部输入适配——办公（飞书/腾讯会议）、RSS、浏览器采集、File Ledger——统一复用现有连接层：`crates/screenpipe-connect`（`connections/` 与 `office/`）、连接 API 与 PRD R12 定义的「连接」入口。授权、账号、范围和同步配置只有一份编辑真源（对应 PRD AC-R13-01），不再出现第二套可编辑的账号/范围/开关。

办公在领域侧归 `knowledge-pack` 的一条来源通道（`office_message` / `office_document` / `office_transcript` / `office_summary`），不新造 `office-pack`；出现独立审核流程、独立权限模型或独立产物类型时再评估拆分。

### D-09：单一 scope 维度，读取侧统一放行

对象、来源和记忆共用一个 `scope` 字段，默认 `personal`；第一批取值 `personal`、`work`（客户资料），办公导入的 scope 由连接决定。落点复用现有字段而不是新增平行概念：来源表的 `dataset_id` 与 `scope_key` 承载 scope 语义，`memories` 只新增 scope 列（可见性、冲突、版本等留到出现第二个消费者时再定）。读取侧的 scope 检查只有一个入口（检索引擎），检索、问答、MCP、聊天、发布投影和文件出口共用，禁止各页面自行过滤。

隔离方式已由 D-11 决定为同库逻辑隔离（落地要求见 D-11，作用域约束见 §2.4）；升级为独立库的条件写在 D-11 的重新评估条件里。

### D-10：文档职责唯一

[roadmap](../roadmap.md) 维护阶段顺序、优先级和进度；本文件维护跨阶段语义、决策和契约冻结顺序；`docs/plans/` 维护实施步骤与验收；`docs/reviews/` 维护就绪与验收结论。同一事实只在一处维护，其余位置只放指针。

### D-11：客户资料采用同库逻辑隔离

客户资料（`work`）与个人数据同库存储，靠 `scope` 字段加检索放行统一隔离，不新建独立数据库或独立 profile。理由：来源表的 `dataset_id` 当前恒为 `local` 且没有第二条写入路径，复用成本接近零；独立库会把 writer、备份和删除传播同时变成两份，而删除传播正是刚在 R6/F02 收口的脆弱链路。

落地要求：

- scope 词表 v1 = `personal`（默认）/ `work`；历史值 `local` 在迁移中归一为 `personal`；办公导入的 scope 由连接配置决定，且这是唯一赋值点。
- 连接级 scope 不足以覆盖同一账号内的客户内容（现行办公对象表只有 provider / account / object_kind / object_id，没有空间或项目归属）：导入必须支持**对象级改判**（按群、空间或文档归属映射，并保留手工标记入口），连接级只作为默认值。判定方式见 §6 O-06。
- 读取放行只有一个入口（检索引擎），检索、问答、MCP、聊天、发布投影和文件出口共用；任何新增出口必须显式声明它按 scope 过滤。
- 备份、恢复、删除传播、retention 和删除后重建索引在同一库内按 scope 一致执行；scope 变化等同来源失效，按 §4 规则 5 处理。
- 受管文件出口（CLAUDE.md / AGENTS.md / Obsidian）与公开发布默认只投影 `personal` 且已审核的内容；`work` 内容进入任何外发出口必须显式授权。

重新评估条件：出现多用户或共享设备、客户合同要求数据物理分离、或需要把 `work` 数据整体搬迁或单独备份交付时，升级为独立库/独立 profile；届时保留同一 scope 词表和放行入口，避免上层重写。

## 2. 分层架构

```text
┌────────────────────────────────────────────────────────────┐
│ screenpipe Core                                            │
│ Object / Memory / Source / Evidence / Relation / Lifecycle │
│ Workflow / Run / Attempt / Review / Search / Backup        │
└────────────┬───────────────────────────────┬───────────────┘
             │                               │
             ▼                               ▼
    Domain Pack（内置）                Adapter（两侧）
    knowledge · personal               输入 Connector：
    research · radar · reading           浏览器采集 · RSS · File Ledger ·
                                         办公平台 · 本地采集
                                       输出 Publication / Export：
                                         Lumina 公众站点 · 受管文件出口
                                         （CLAUDE.md / AGENTS.md / Obsidian）
```

读法：Domain Pack 处理领域语义；输入 Connector 只负责授权与取数，输出 Adapter 只负责单向投影。两者都不持有正式状态，也不得绕过 Core 写入。

### 2.0 现状锚点：每个 Core 原语复用还是新建

实现前先对表；表中“动作”是唯一允许的做法，避免把“扩展”做成“重写”。

命名：本文件统一把这一领域称为 `knowledge-pack`；引用现行代码标识时保留 `brain` / `brain_*` 原样（模块 `crates/screenpipe-engine/src/brain`、`crates/screenpipe-db/src/db/brain`、`brain_jobs` 等），它们是这一领域的实现命名，不新建模块、不重命名目录。正文不再交替使用两个名称。

| Core 原语 | 现状实现 | 本框架动作 |
|---|---|---|
| Workflow / Run / Attempt | `crates/screenpipe-core/src/tasks/mod.rs`（TaskDefinition / TaskRun / TaskAttempt / TaskEvent / TaskControl / TaskOwnerState）；`create_tasks.sql` 的 `task_definitions` / `task_runs` / `task_attempts` / `task_events` / `task_legacy_map` / `task_owner_state`；门面 `crates/screenpipe-engine/src/tasks/mod.rs` | 扩展：新包按 D-07 注册；legacy `brain_jobs` 桥在迁移收尾后删除 |
| Source / 来源修订 | `brain_sources` / `brain_source_revisions`；`SourceKind` 闭集；`SourceRef{source_uid, dataset_id, revision, locator}` | 扩展：kind 由包注册；`dataset_id` 承担 scope（D-09） |
| Source Evidence | `brain_source_revisions.excerpt` + `brain_dependencies(consumer_kind, consumer_id, field_path, source_uid)` | 复用 |
| Relation | 只有派生依赖 `brain_dependencies`，无通用关系表 | 新建最小契约（§6 O-02），不做图数据库 |
| Review / 版本 | `brain_knowledge` / `brain_knowledge_versions` / `brain_rejections` + `engine/brain/registry` 校验器 | 复用 |
| 生命周期 / 删除传播 | `brain_deletions` / `brain_cleanup_items(exit, target, state)` / `brain_tombstones` | 扩展：每个新出口必须登记 exit |
| 对象信封 | 无通用表；各域自己带版本列（`brain_work_units`+revisions、`brain_knowledge`+versions） | 新建最小信封；不重写既有表 |
| 情境记忆 | `memories` + `memories_fts`；`SourceKind::Memory` | 扩展：只新增 scope 列 |
| 长期记忆（semantic / procedural） | `brain_knowledge` / `brain_knowledge_versions`（已带版本、审核、暂停；无过期与冲突标记） | 复用：不搬到新表；Core 只统一检索入口与生命周期规则（§2.2 落地解释） |
| Action Intent / Completion Record | 无 | 新建（P3，`personal-pack`）；不得与 Source Evidence 混用（§2.3） |
| 外发出口 | `crates/screenpipe-engine/src/external_memory_sync.rs` → `~/.claude/CLAUDE.md`、`~/.codex/AGENTS.md`、Obsidian；目的地在 `screenpipe-connect::connections::{claude_code, codex}` | 复用并登记进出口清单 |
| Connector / 授权 | `crates/screenpipe-connect`（`connections/`、`office/`）、连接 API、「连接」入口 | 复用：新输入一律走同层 |

### 2.1 Core 原语

Core 只保证：

- 对象信封：`id`、`type`、`schema_version`、`revision`、`status`、`scope`、`payload`；
- **Memory 原语**：记忆记录、作用域、有效期、来源引用、可见性、召回和生命周期；
- Source 和来源命名空间；
- Source Evidence / Provenance、Relation 和版本有效性；
- Workflow、Run、Attempt、取消、重试、恢复和幂等；
- Review/Approval、质量门、发布边界和审计；
- 检索、权限、删除传播、备份和恢复。

`WorkUnit`、`KnowledgeVersion`、`ResearchTask`、`Claim`、`Goal`、`Quest`、`Article`、`DailyReport` 等是 Domain Pack 或外部适配器对象，不成为 Core 的固定产品特判。Domain Pack 可以定义记忆的语义和生成器，但不能绕过 Core 的记忆存储、检索、权限和生命周期。

### 2.2 记忆是 Core 能力，领域包提供内容

记忆不是另起一个 `memory` 产品。它是 Core 提供的跨领域能力，所有需要保留上下文的 Domain Pack 都可以使用；`knowledge-pack` 只是当前最先消费它的领域包，而不是记忆的所有者。

Core 统一承载两类记忆：

1. **情境记忆（episodic memory）**：保留“我在什么时候、什么应用、什么对话或会议里看到/听到什么”。当前兼容 `memories` 表及其 FTS 路径，作为可检索的 `SourceKind: memory`；它可以被引用，但默认不等于已确认知识。
2. **长期记忆（semantic/procedural memory）**：由领域包从来源证据中提炼的稳定对象。`knowledge-pack` 的 SOP、decision rule、exception playbook 是第一批实现，但研究结论、阅读主题、个人偏好和雷达趋势也可以成为不同类型的长期记忆。

每个领域包只负责三件事：定义自己的记忆类型和生成规则，提交带证据的候选，解释领域语义。Core 负责统一存储、索引、召回、作用域/权限、版本、过期、删除传播和冲突标记。

落地解释：情境记忆当前落在 `memories` 表，长期记忆当前由 `knowledge-pack` 的知识表承载。Core 的“统一管理”指统一检索入口、放行判断、删除传播和生命周期规则，**不要求把长期记忆搬进新表**；P2 只新增 scope 列与放行入口，过期与冲突标记等出现真实需求再定。

| 领域包 | 可贡献的记忆 | 默认处理 |
|---|---|---|
| `knowledge-pack` | SOP、决策规则、异常处理经验 | 候选 → 审核 → 长期记忆 |
| `personal-pack` | 偏好、承诺、目标关联和复盘线索 | 正式 Goal/Quest 仍以 Personal OS 文件为真源 |
| `reading-pack` | 文档主题、Claim、阅读批注和资料关联 | 必须保留原文来源与版权边界 |
| `research-pack` | 假设、研究结论、比较结果和 Artifact 线索 | 随研究周期和 Artifact 版本管理 |
| `radar-pack` | 信号、趋势、重复出现的事件模式 | 默认短期/可过期，不自动升级为长期记忆 |

查询时由统一 Answer/Brain 检索合并三条路线：原始/办公来源、情境记忆、已发布且有效的领域记忆。返回结果携带 `EvidenceRef`、来源时间、版本和可回放路径；上下文不足、来源失效或记忆冲突时返回不确定状态，而不是静默生成“记忆”。

因此嵌入边界是：采集层提供证据，领域包提出记忆，Core 编译并管理记忆，聊天和 Pipes 只消费经过检索授权的记忆。Personal OS 文件、Lumina 文章和 Infinitum 信号可以贡献记忆，但不自动成为 screenpipe 的长期记忆。

### 2.3 两类证据

- **Source Evidence**：证明事实、判断或产物来自哪个来源、版本和片段。
- **Completion Record**：证明用户确认的行动结果或复盘记录。

二者可以通过 Relation 关联，但不能互相替代。后台 Run 成功也不等于行动完成或知识发布。

### 2.4 作用域与客户隔离

- `scope` 是对象、来源和记忆的一等字段，默认 `personal`；第一批取值 `personal` 与 `work`（客户资料），办公导入的 scope 由连接决定。
- **硬约束**：个人知识入口不自动纳入客户正文、`work` 仓库或 Agent 记忆正文；单用户同样保留范围约束，不因为“只有一个人用”就省略。
- 检查点只有一个：检索引擎入口。检索、问答、MCP、聊天、发布投影和文件出口共用同一放行判断，禁止只在某个页面过滤。
- 作用域变化（连接缩范围、断开授权）等同来源失效：立即阻止相关旧运行提交，并纳入同一删除链；导入类来源保留不含正文的抑制记录，防止重新导入。
- 落点复用现有字段（`dataset_id` / `scope_key`），`memories` 只补 scope 列；隔离方式已由 D-11 决定为同库逻辑隔离（不新建独立库/profile），具体迁移列与词表归一在 P2 冻结时确定。

## 3. 领域包与适配器

| 扩展 | 负责什么 | 真源/边界 |
|---|---|---|
| `knowledge-pack` | SOP、决策规则、异常处理经验、Work Unit、知识候选、知识版本、引用问答，以及办公来源通道 | 使用 Core Memory；长期记忆必须审核、版本化和可回溯；办公授权归连接层（D-08） |
| `personal-pack` | Inbox、Goal、Quest、Review、行动关联 | Personal OS File Ledger 持有正式文件状态 |
| `research-pack` | 关注目标、研究任务、Claim、比较、Artifact、研究反馈 | screenpipe Core 持有运行/审核/发布边界 |
| `radar-pack` | RSS、信号、基础聚合、候选事件 | Infinitum 提供输入和预处理，不持有复杂综合结果 |
| `reading-pack` | 网页资料、正文、阅读、批注、摘要和翻译 | 新采集直接进入 screenpipe；Lumina 旧数据受控导入 |
| Connector 层 | 办公平台、RSS、浏览器采集、File Ledger、其它外部输入的授权与取数 | 复用 `crates/screenpipe-connect` 与「连接」入口；授权/范围只有一份编辑真源（D-08） |
| Publication / Export Adapter | 把已审核 Artifact/Knowledge Revision 或记忆投影到外部展示面或受管文件 | 单向、以 Core 版本为准；出口必须登记进删除链与设置出口清单 |

## 4. 数据与执行规则

1. 新输入先物化为带来源、版本和完整度的对象，再进入 Workflow；来源服务不可用不能伪装成零结果。
2. 每种正式状态只有一个拥有者。扩展可以提出候选或变更，不能绕过 Core 写正式状态。
3. Workflow 是统一执行模型；`knowledge-pack`、Infinitum 和 Lumina 不得各自维护第二套任务调度或 AI 任务真源。新增内置包按 D-07 注册种类，不得自建调度循环。
4. 模型输出必须区分原始事实、推断、候选知识和已确认知识；不能因为模型 confidence 高就自动发布。
5. 删除和来源失效必须阻止旧任务提交，清理派生正文、索引、缓存和外部投影；发布撤回不等同于删除外部已经保存的副本。
6. Lumina 的公众发布必须显式授权，按 Artifact 版本投影；不得默认公开原始采集内容或受版权限制的全文。
7. 新增包不得进入采集/编码热路径；所有写入走既有 SQLite 协调入口，不新增第二个写者（见 `AGENTS.md` 热路径约定）。
8. 任务所有权按类型分批切换：同一时刻一个调度所有者，停旧触发 → 收敛在途任务 → 原子切换 `task_owner_state` → 启动新触发，任何一步都能回退且不双跑、不复活已删内容。

## 5. 契约冻结顺序与依赖

本节只声明**冻结顺序与依赖**，不是进度表；阶段进度见 [roadmap](../roadmap.md)。

| 必须先冻结 | 依赖 | 触发时点 |
|---|---|---|
| 对象信封 + scope 语义 | D-11（已决定：同库逻辑隔离） | 接入第二个采集包（reading/radar 任一）之前 |
| 对象级 scope 判定规则 | O-06 | scope 迁移批内（信封冻结同期） |
| 任务种类注册清单 | D-07 | 新包第一次新增种类之前 |
| 出口清单与撤回/删除边界 | O-05 | 任何新外发出口或公开发布启用之前 |
| File Ledger 读写契约 | O-04 | `personal-pack` 接入之前 |
| 研究场景、输入指纹与回归口径 | O-03 | `radar-pack` / `research-pack` 开工之前 |

## 6. 未决项

| ID | 待决定 | 最晚时点 | 判定方式 |
|---|---|---|---|
| O-01 | 客户资料的隔离方式 | 已收口 | 已由 D-11 决定（2026-09-10）为同库逻辑隔离；判定用例并入 D-11 落地要求与 P2 验收 |
| O-02 | 通用 Relation 的最小范围（是否只有派生依赖 + 显式引用） | 冻结信封之前 | 以 reading-pack 与 radar-pack 各一条 输入→Workflow→产物 路径验证；不得仅以“JSON 可存储”作为通过条件 |
| O-03 | Infinitum 预处理与 screenpipe 复杂综合的边界、输入指纹与结果复用；旧日报回归样本口径 | `radar-pack`/`research-pack` 开工前 | 用历史日报建立信号包/简报对照集，覆盖历史比较、证据引用、重跑与反馈 |
| O-04 | Personal OS 的文件根、唯一写入入口、单写者与并发/失败恢复方式 | `personal-pack` 开工前 | 隔离账本上完成读写往返、并发修改与异常恢复验证 |
| O-05 | 发布出口协议：可见性、版权/隐私、撤回、更新与删除边界（撤回≠清除外部已保存副本），以及新出口如何登记进删除链与设置出口清单 | 任何公开发布或新出口启用前 | 发布 → 更新 → 撤回 → 删除 → 目标不可用 五场景验证，且不出现第二事实源 |
| O-06 | 同一连接内的对象级 scope 判定：按群/空间/文档归属的导入规则映射，以及手工改判入口 | scope 迁移批内（信封冻结同期） | 同一连接里含客户内容的文档不得出现在 personal 检索或任何外发出口，同连接的个人文档仍可检索；改判后旧索引与派生内容按 §4 规则 5 处理 |

## 7. 当前不做

- 不再维护独立 MIT 拆分版或追赶历史 MIT 基线。
- 不整体复制 zhiji、zhihuan、Infinitum 或 Lumina 的数据库、页面和 Worker。
- 不建设开放式第三方插件市场，不做动态类型注册表或未知种类兜底。
- 不新建第二套调度、任务真源或审批状态；不在 Core 写来源产品特判。
- 不把 Personal OS 的文件真源迁入 screenpipe。
- 不把通用 Relation 做成图数据库或独立图谱服务。
- 不默认启用全文公开发布、双向办公写入、团队协作、云同步、计费或商业分发。

## 8. 文档职责、变更记录与承接关系

| 文档 | 唯一职责 |
|---|---|
| [roadmap](../roadmap.md) | 阶段顺序、优先级、当前进度；未开工的阶段不提前标为完成 |
| 本文件 | 跨阶段职责、Core 语义、设计决策与变更原因、契约冻结顺序；不复述进度与验收 |
| `docs/plans/` | 开工阶段的实施步骤、依赖、验收与完成状态 |
| `docs/reviews/` | 阶段就绪与验收结论（本框架的评审见 [readiness 报告](../reviews/personal-workbench-integration-framework-readiness.yaml)） |

承接与取舍（相对归档 zhiji v0.5，`docs/archive/zhiji/trd/product-integration-framework.md`）：

- 归档的 D-10–D-17 收敛为本文件 D-02–D-06；D-01–D-09 中与 zhiji 独立产品身份、MIT 来源冻结相关的部分不再适用。
- 归档 §3.1 的 `data_scope` 隔离规则恢复为 §2.4；归档 O-01–O-10 收敛为 §6 的 O-01–O-05，逐阶段补齐。
- 归档 §6 的“阶段计划携带项”（所属决策与未决项、输入版本、复用分类、数据拥有者、契约变化、最小可用流程、验收证据、回退方式）继续适用于 `docs/plans/`。
- 归档 §9 的文档职责与变更记录恢复为本节；本次同时移除了本文件此前对 P1–P5 进度与验收的复述（阶段权威交回 roadmap，见 D-10）。

| 日期 | 版本 | 变更与原因 | 影响 / 验证状态 |
|---|---|---|---|
| 2026-09-10 | 1.0 | 承接 zhiji 归档路线的初版：D-01–D-06、分层架构与领域包、数据与执行规则、P1–P5 计划 | 方向已确认；接口细节留待各阶段 |
| 2026-09-10 | 1.1 | 针对 readiness 评估（DR-MOD-001/002、DR-IFACE-001、DR-DATA-001、DR-SEC-001、DR-PROC-001/002、DR-DOC-001）调整：新增 D-07 扩展点唯一路径、D-08 连接层归属、D-09 单一 scope、D-10 文档职责；新增 §2.0 现状锚点表、§2.4 作用域与客户隔离、§5 契约冻结顺序、§6 未决项、§8 承接记录；§2.1/§3 标注复用现状、§4 增补规则 7/8；移除 P1–P5 进度与验收复述 | 文档层结论；隔离方式当时待人工决定，P2 计划编制前解决；实现状态未变（still not_started） |
| 2026-09-10 | 1.2 | 用户决定同库逻辑隔离，记为 D-11（含落地要求与重新评估条件）；O-01 收口，编号保留不复用；§2.4 与 §5 依赖同步更新 | P2 可据 D-11 安排 scope 迁移与放行入口；O-01 判定用例并入 P2 验收 |
| 2026-09-10 | 1.3 | 基于实现核对（knowledge-pack 已落地部分与框架的出入）收敛：统一领域命名为 `knowledge-pack`（仅代码标识保留 `brain*`）；§2.2 增加“落地解释”（长期记忆不搬新表）；§2.0 补“长期记忆”与“Action Intent / Completion Record”两行；D-11 补对象级 scope 改判要求并新增 O-06 与对应冻结点 | 文档层结论；O-06 归入 P2 scope 迁移批，判定用例已声明；实现状态未变（still not_started） |
