<!-- screenpipe — 本地优先的个人知识库 -->

---
id: prd-personal-brain-local-first
type: prd
status: draft
created_at: 2026-09-05
updated_at: 2026-09-05
sources:
  - "/Users/shawn/Documents/inspiration/startup/research/company-brain-poc-trd.md"
  - "zh-local 分支现状调研：记忆系统（memories / activity_ledger / activity_history）、检索管线、AI Provider 层"
related:
  - VISION.md
  - AGENTS.md
---

# PRD: 个人大脑本地先行版（Personal Brain Local-First）

## 1. 背景

Company Brain POC TRD（见 sources）定义了一条「工作轨迹 → Work Unit → 团队知识 → 混合检索问答 → 反馈更新」的知识编译管道，但其主形态是云端 Team Brain + 多人协作。本 PRD 将该管道的**本地可裁剪子集**落到 screenpipe（zh-local 分支）上，做成单机、个人维度的先行版，用于验证：真实工作轨迹能否在无人值守下沉淀为可检索、带证据、可维护的个人知识。

现状基础（已存在，不需要新建）：

- 采集与全文检索：OCR / Accessibility / 音频采集 + SQLite FTS5（`/search`）。
- 工作解释的两层雏形：确定性 activity ledger（任务区间 + 证据表 + 级联删除）与 LLM 生成的 activity history（叙事 timeline）。
- 事实型记忆：`memories` 表 + REST/MCP CRUD + 外部文件同步（CLAUDE.md / AGENTS.md / Obsidian）。
- 无人值守 AI 任务运行时：`run_background_pi`（证据包预注入、工具禁用、校验回修循环）。
- AI 接入：AI Preset（OpenAI-compatible / Ollama / 自定义端点），local-only 构建已移除账号流与云端 provider。

关键产品决策（已确认）：

1. **模型接入保留现有 AI Preset 形态**，不强制本地模型；用户自行选择模型服务（本地或云端 API 均可）。
2. 数据与知识沉淀**全部本地**；只有模型调用内容按用户已有 Preset 配置发往所选模型服务。
3. 记忆层 **Provider 化**：记忆只是混合召回的一路来源，可外接外部记忆引擎。
4. 知识沉淀**个人维度**：无团队、无 ACL，审核为个人自审。

## 2. 目标

- 打通本地闭环：工作轨迹 → Work Unit → 个人知识对象（SOP / Decision Rule / Exception Playbook）→ 自审发布 → 混合检索 → 带引用问答 → 反馈回流。
- 记忆层抽象为 MemoryProvider：默认实现行为与今天完全一致，支持外接 Mem0 / Graphiti 等 HTTP 记忆引擎。
- 检索从「多路并行 + 时间戳排序」升级为多源混合召回（FTS + 向量 + 记忆 + 知识 + 语义层）。
- 沿 zh-local 方向完成本地化裁剪：团队/云端组件在 local-only 构建下不可达。

## 3. 非目标

- 不做云端上传、团队/组织协作、ACL、多用户、ConsentPolicy、计费。
- 不做 Skill 自动执行与高风险操作（发消息、改数据库、提审批）；Checklist 执行视图、Case 发布、Skill 编译属后续阶段。
- 不强制本地模型，不新增模型推理基础设施（DGX Spark / Qwen 常驻服务不在范围内）。
- 不做 Methodology（TRD 中的 P3 类型）。
- 不改动采集 / 编码 / DB 写入热路径；知识管道全部为离线批处理。
- 不替换现有 AI 接入形态（AI Preset 即唯一模型入口）。

## 4. 用户与场景

用户：单人知识工作者（本机使用，无协作方）。

- **场景 A（后台沉淀）**：正常工作 → 定时/手动触发抽取 → 生成 Work Unit → 聚合编译知识候选 → 用户在审核台自审（编辑/发布/驳回/废止）。
- **场景 B（检索问答）**：问「遇到 X 异常怎么处理」「上周为什么延期」→ 混合召回命中已发布知识与原始证据 → 返回带引用、带版本的回答；低置信时明确拒答。
- **场景 C（记忆外接）**：在设置中配置外部记忆引擎 → 混合召回时合并其结果并标注来源；引擎不可用时该路静默降级。
- **场景 D（删除）**：删除某段屏幕记录或某条记忆 → 证据、Work Unit、知识版本、索引（FTS/向量）全链清理，无残留。
- **失败处理**：模型超时或输出不合法 → 重试一次 → 进入待处理队列，不静默丢弃也不产生脏数据；外部记忆 provider 失败 → 跳过该路并记录诊断。

## 5. 需求

### R1 工作轨迹与 Work Unit

- 新增 `work_unit.v1` 结构化输出：task / inputs / actions / decisions（条件→动作）/ exceptions / outputs / result / evidence_ids / confidence。
- Work Unit 以 activity ledger 的任务区间为锚自动生成；显式「开始/结束工作会话」标签为后续增强（见开放问题 4）。
- 幂等：同一「区间 + prompt 版本 + schema 版本」重复执行不产生重复 Work Unit。
- 校验：输出过 JSON Schema；evidence_ids 必须真实存在；校验失败自动回修一次，仍失败进待处理队列。
- 活动叙事（activity history）持久化从 Tauri 加密 store 迁移到 SQLite，作为 Work Unit 的同库输入。

验收标准：
- 一天真实数据中 ≥80% 的任务区间产出合法 Work Unit。
- 进程异常退出或输出非法时，不产生重复或无来源的 Work Unit。
- 删除一个 frame，引用它的 evidence / Work Unit 关联被级联清理。

### R2 个人知识编译与自审

- 第一版仅三种 P0 知识类型：SOP、Decision Rule、Exception Playbook。
- 知识对象必带：type / content（结构化）/ derived_from_work_units / evidence_ids / status / version / confidence / review_after / supersedes。
- 状态机（个人自审版）：extracted → candidate → published / deprecated；支持修订出新版本（version + supersedes），不覆盖历史。
- 相似知识冲突（去重命中）不自动合并，进人工确认。
- 审核台：候选列表、查看 evidence 来源（可回链到 frame 级）、编辑、发布、驳回、废止。
- 系统不自动把候选发布为知识；发布必须经过用户确认。

验收标准：
- 同一流程 ≥3 个工作会话的 Work Unit 能编译出 1 份含共同步骤和异常分支的 SOP 草稿。
- 每个已发布知识的来源可回链查看；可废止并回看历史版本。

### R3 混合召回

- `/search` 的 All 召回升级为多源并行：timeline（OCR/A11y/音频 FTS5）、semantic parsed items、memory（经 MemoryProvider）、knowledge（FTS5 + 向量）、向量召回（文本嵌入）。
- 合并策略从纯时间戳排序升级为 RRF / 加权合并；每路召回带诊断标记（沿用 activity-summary 的 `searched_endpoints` 模式）。
- 任一来源禁用、超时或失败时跳过该路，整体召回不报错。
- 时间范围 / app / 内容类型过滤下推到各路。
- 多源清单不含云端搜索腿：`include_cloud` 参数已随瘦身删除（见 §13.4）。

验收标准：
- 禁用外部记忆 provider 后 `/search` 行为正常，仅诊断标记显示该路未命中。
- 固定语料 + 固定查询的快照回归测试通过（防止合并策略静默劣化）。

### R4 MemoryProvider

- 定义 Provider 接口：write / search / delete；每条结果带 provenance（provider 标识 + 原始引用）。
- 默认实现 `LocalSqliteProvider` 包装现有 `memories` 表，现有 REST `/memories*`、MCP `update-memory`、外部文件同步（出口）行为不变。
- HTTP provider 对接外部记忆引擎：Mem0（user-scoped）为首选验证对象，Graphiti 预留接口；连接配置（URL / key / enabled）放设置。
- 多 provider 结果在召回层合并去重，并标注来源。
- 跨设备同步链路已随瘦身整体删除（后端 sync 模块与 MCP synced 工具不复存在；表内 sync 字段保留不迁移，见 §13.3）；`LocalSqliteProvider` 不承担同步职责，跨设备一致性由外接 provider 与文件生态承担。

验收标准：
- Provider 化作为纯重构独立提交，现有行为与测试零回归。
- 配置外部 provider 后，召回结果包含其条目且来源标注正确。

### R5 带引用问答（/answer）

- 新增问答端点：检索 → 组装证据包 → 调用用户 AI Preset（普通 completion，非 agent 循环）→ 返回：结论、来源列表（evidence 引用 + 知识 id 与版本）、置信度、needs_confirmation。
- 回答必须可点开每条来源；无足够证据时返回「证据不足」，不得输出无来源的结论性答案。
- 高风险/低置信问题走 needs_confirmation 路径，而不是模型补全。

验收标准：
- 每个回答的来源均可回链（知识版本或 frame）；frame 被保留策略清理后显示为「已归档引用」（见 §13.1）。
- 构造无证据查询可稳定触发拒答路径。

### R6 数据生命周期与隐私

- 删除传播全链：frames → evidence → work_units → knowledge_versions → 索引（FTS / 向量），不留孤儿副本。
- 传播区分两类触发：用户主动删除与保留策略自动清理；已发布知识的 frame 文本证据豁免于保留清理（见 §13.1）。
- 原始采集数据不出本机；发往外部的仅两类：模型调用内容（按用户 AI Preset 配置）、记忆条目（按用户显式配置的 provider）。
- 团队/企业组件（gateway、MCP team-* 工具、team-memory 格式）已随瘦身整体删除。
- 可审计：能列出哪些数据类别会发往哪些外部端点。

验收标准：
- 删除后 FTS 与向量查询无残留命中。
- 设置页可展示外部数据流向清单。

## 6. 非功能需求

- **热路径零回归**：新增 worker 全部离线批处理，不得破坏 <20% CPU / <3GB RAM 的发布目标；回归需在 PR 中测量说明。
- **沉淀 SLA**：Work Unit 在证据就绪后 15 分钟内完成（可配置）；普通问答 80% 在 60 秒内返回。
- **文案与 prompt 中文优先**：延续 c9456cb25 的约定，用户可见文本与生成 prompt 使用简体中文。
- **平台**：macOS 先行验证；实现遵守仓库跨平台抽象，不引入平台私有依赖。
- **可观测**：抽取成功率/失败原因分布、各路召回命中与耗时、问答引用使用率、外部 provider 失败率。

## 7. 依赖与约束

- 源 TRD：`company-brain-poc-trd.md`（本 PRD 仅取其本地可裁剪子集；云端、团队、模型自部署章节不适用）。
- 仓库约束：AGENTS.md 工具链（bun / cargo、native build queue、测试边界、PR 需附评测结果）；VISION.md 的 no-feature-creep 约束（本 PRD 全部能力服务 Rewind / Ask）。
- 现有组件依赖：`activity_history.rs`、PiExecutor / `run_background_pi`、`screenpipe-core::memories::external_sync`、`screenpipe-db`（FTS5、sqlite-vec 0.1.3）、`/search` 多路并行骨架。
- 分支上下文：zh-local 已完成账号流移除与云 provider 拒绝（4dd640d45、f1186902），本 PRD 延续该方向。

## 8. 验收与成功指标

个人版改编自 TRD §14，最小验收集：

- 完成 ≥10 个真实工作会话的数据沉淀。
- 产出 ≥1 份可用的 SOP 草稿，且识别出 ≥1 个现有笔记/文档未覆盖的流程变体。
- 混合召回下，异常处理类问题能同时返回已发布知识与原始证据双来源。
- 每个回答带可回链来源；无依据回答率为 0（拒答路径生效）。
- 删除传播后索引无残留；未发生数据外发超出第 5 节 R6 声明的两类边界。
- 用户（本人）愿意持续使用问答或审核台（主观留存判断）。

## 9. 开放问题

1. **嵌入方案**：起步用 AI Preset 的 `/embeddings`，还是直接引入本地 ONNX embedder？（倾向前者，后续评估）
2. **审核台位置**：设置页新增「知识」区，还是首页活动区旁入口？
3. **/answer 是否同时暴露为 MCP 工具**——已裁决（2026-09-05）：是，Phase 3 实现，依据见 §13.5。
4. **显式工作会话**（手动开始/结束 + 任务标签）是否进入第一版，还是纯自动区间锚定？
5. **数字克隆方向**（`brain-search.ts` 的 `clone:*` tags）与新知识对象如何归并——已裁决（2026-09-05）：冻结新增，等待 §12.2 触发信号（§13.6）。
6. **activity history 迁移**：Tauri store 旧数据的迁移与回退策略细节。
7. **对话式修订知识内容**：审核台「编辑」是否加入对话式修订（跟 AI 聊着改草稿）？倾向作为 v1.5 快速跟进项，见 §12.4。

## 10. 风险

| 风险 | 影响 | 缓解 |
|---|---|---|
| 模型结构化抽取质量不足 | Work Unit 不可用 | 降低粒度（按半天聚合）；Schema 校验 + 回修；换模型档位实测 |
| activity history 迁移回归 | 用户叙事数据丢失 | 独立提交、迁移前后快照对比、保留回退读取 |
| RRF 权重无评测依据 | 召回质量拍脑袋 | 先建 30–50 条查询评测集与人工标注，再调权 |
| 个人自审无专家把关 | 错误知识入库且被引用 | 强制 evidence 链接、版本与废止机制、低置信拒答 |
| 范围蔓延 | 偏离 VISION 稳定性原则 | 按 Phase 分期；最小闭环止于「检索 + 问答 + 自审」，Memory 外接与增强项后置 |

## 11. 设计输入（给后续 TRD / 执行计划）

- **分期建议**：P0 Schema 与裁剪 → P1 Work Unit 抽取 worker（含 activity history 迁库）→ P2 知识编译与审核台 → P3 MemoryProvider + 混合召回 + /answer → P4 反馈闭环与删除传播。其中 P3 的 Provider 纯重构可提前并行。存量功能冲突的裁决与执行时机见 §13。
- **关键落点**：`apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs`（抽取模式复用）、`crates/screenpipe-engine/src/routes/search.rs` 与 `crates/screenpipe-db/src/db/search.rs`（多路合并改造）、`crates/screenpipe-core/src/memories/`（Provider trait）、`crates/screenpipe-db/src/migrations/`（新表 + 级联 trigger）、`packages/screenpipe-mcp`（工具暴露）。
- **存储决策**：Work Unit / 知识对象入 SQLite（与 memories / ledger 同库），配 FTS5 虚表与 sqlite-vec 向量列；删除传播沿用 activity ledger 的级联 trigger 模式。
- **测试策略**：`cargo test -p screenpipe-db`（表与触发器）、引擎 route 测试、前端走 browser-mock loop（不 build Tauri 验 UI）、召回合并用固定语料快照测试；PR 附评测命令与结果。
- **类型系统约束**：三种 P0 类型实现为内部注册表形态，pipeline 只依赖注册表；演进路径见 §12。

## 12. 知识类型演进路径

v1 仅三种 P0 类型（见 R2）。本节约束其后续演进方向与节奏，防止两个方向的失败：过早开放类型定义导致编译质量失控，以及硬编码到无法演进。

### 12.1 v1 设计约束（TRD 必须遵守）

- 三种类型实现为**内部注册表**：每个类型 = `{json schema, prompt 模板, 校验器, 渲染器}` 四件套，pipeline 只依赖注册表，不感知具体类型。后续演进是加条目，不是改管道。

### 12.2 第一次演进：静态配置化

- 注册表外置为配置文件；用 Checklist（SOP 派生）与 Case 作为第一批「配置新增」类型验证机制。
- 硬底线：
  - 类型必须约束在**有限字段词汇表**内（title / steps / conditions / symptoms / escalation 等通用槽位），不允许任意自由字段，否则召回、去重、审核台渲染全部退化为通用文本，知识对象失去相对原始记忆的增量。
  - 类型定义本身版本化，schema version 进入 Work Unit 幂等键；类型有废止路径。

### 12.3 第二次演进：AI 对话创建（起草向导，非自由创建）

- 流程：用户自然语言描述想沉淀什么 → AI 提案类型定义（schema + prompt + 适用场景）→ 用户确认 → 影子模式（只产出候选、不进召回、积累样本人工确认）→ 转正。
- AI 创建的类型**绝不自动发布知识**。
- 实现可复用 pipe 形态先例（pipe.md = prompt + frontmatter 的 AI 任务包）。

### 12.4 可提前独立项

- **对话式修订知识内容**（非类型）：审核台「编辑」支持对话式修订草稿。成本低于类型配置化，针对「草稿不满意但懒得手改」的高频痛点，可作 v1.5 独立评估（开放问题 7）。

### 12.5 演进触发信号

满足任一即启动 §12.2 配置化：

1. 第 4、5 个类型需求真实出现（非想象需求）；
2. 出现非流程类知识沉淀需求（某个人、某个项目状态、某类会议模式）；
3. 固定类型运行 3–4 周后，编译候选中稳定出现「内容有价值但塞不进现有类型」的模式。

## 13. 存量功能冲突裁决

以下裁决于 2026-09-05 与用户确认，处理现有功能与本 PRD 迭代的冲突点。

**执行状态总览（2026-09-07，瘦身收尾后）**：§13.2 / §13.3 / §13.4 / §13.6 的裁剪项已全部落地（多数以物理删除实现，严于裁决原案）；§13.1 待 Phase 0 evidence schema 定稿时落实；§13.5 按计划 Phase 3 实现。

### 13.1 保留策略与知识证据链（影响 Phase 0 schema，最重要）

- 事实：retention `All` 模式批量删除 frames 与转写（`crates/screenpipe-engine/src/retention.rs`），级联触发器连带清理 activity evidence；而知识的 evidence_ids 指向 frame，自动清理会让来源回链腐烂。
- 裁决：混合方案——**已发布知识引用的 frame 行及其 OCR/转写文本豁免于保留删除，视频文件不豁免**（文本行是 KB 级小数据，视频才是磁盘大头）；候选/已废止知识不豁免，编译时固化证据摘要（excerpt + app + timestamp + frame_id），frame 清理后来源展示为「已归档引用」。
- 实现：retention 批量删除时 join 豁免集合（published 知识 evidence 集合）；批量、低频，不碰热路径。

### 13.2 商业档位钳制短路

- 事实：`FREE_ACTIVITY_HISTORY_HOURS=24`、`settings_restrict_activity_history`、`HistoryAccessPolicy` 残留于 activity_summary / streaming 等路由，会把知识编译时间窗人为压到 24 小时并截断 preflight 证据。
- 裁决：local-only 构建下策略短路为无限制；**不物理删除代码**，降低日后合并上游的冲突面。Phase 1 落地。
- 状态（2026-09-07）：瘦身阶段已确认短路到位——`activity_history_is_restricted` 恒返回 `false`，钳制实际不生效；Phase 1 仅需决定是否删除残留机器。

### 13.3 memories 跨设备同步

- 事实：同步链路依赖云端中继（S3 上传/下载）与账号 JWT（`sync/client.rs`），账号流已在 zh-local 移除。
- 裁决（用户确认不依赖跨设备，2026-09-05）：同步代码路径移除；`sync_uuid` 等表字段保留不迁移；`LocalSqliteProvider` 不承担同步职责。跨设备一致性由外接记忆 provider 与外部文件生态承担。
- 状态（2026-09-07）：已落地——`screenpipe-core/src/sync/` 模块与 MCP `synced-devices` / `search-synced-content` 工具已整体删除。

### 13.4 云搜索腿与云代理

- 裁决：`include_cloud` 云腿移除，R3 多源清单不含 cloud；`/v1/chat/completions` 云代理下线（其存在理由——pi 子进程隔离云端 JWT——在 local-only 下不成立）。
- 状态（2026-09-07）：已落地——`include_cloud` 参数已删除；云代理未留 404 壳，路由直接改走 `local_chat` 本地 OpenAI-compatible 网关（严于裁决原案）。

### 13.5 知识出场通道收敛（关闭开放问题 3）

- 裁决：外部文件同步（CLAUDE.md / AGENTS.md / Obsidian）只管 memories；知识只经 MCP /answer 一条通道出场，不进文件 dump，避免版本与审核状态多路失控。**/answer 确定暴露为 MCP 工具**（Phase 3 实现）。

### 13.6 其他裁决

- **调度**：POC 不上复杂队列，建带优先级的串行执行器（交互查询 > 实时抽取 > 批量编译），Phase 1 落地。
- **`ocr_text_embeddings` 死表**：已于瘦身阶段 drop（create migration 删除 + drop migration 补入），Phase 3 无需处理。
- **activity ledger 语义**：work unit 锚定尊重 `user_locked` 的用户手动归类；抽取作为新 producer 写独立 watermark。Phase 1 设计约束。
- **内容类型枚举扩展**：`SearchContentType` 新增 Knowledge 按跨层扩展处理（DB、pipe 权限、PII 过滤、MCP 工具说明、TS wrapper、缓存 key 同一提交族完成）。Phase 3。
- **gateway / team-memory / team-* MCP**：已随瘦身整体删除（crate 与工具不复存在，严于原定的编译期 feature-gate 方案）。
- **数字克隆 `clone:*` 方向**：冻结新增，等待 §12.2 触发信号。
