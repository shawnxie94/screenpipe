---
id: plan-personal-brain-correctness
type: execution_plan
status: approved
created_at: '2026-09-08'
updated_at: '2026-09-08'
base_commit: 5fcacf6b1a51bfaa0003e9e9907b608e80bad568
reviewed_head: 0a83343ac7e75cc67effa3dae4430bcb4dd825cf
scope_revision: 2
sources:
- docs/prd/personal-brain-local-first.md
- docs/trd/personal-brain-local-first.md
- docs/plans/personal-brain-local-first-execution-plan.md
- docs/reviews/personal-brain-implementation-readiness.yaml
- docs/reviews/evidence/personal-brain-implementation-review/manifest.json
- AGENTS.md
- VISION.md
- crates/screenpipe-core/src/pipes/mod.rs
- crates/screenpipe-engine/src/pipe_store.rs
- crates/screenpipe-engine/src/brain/worker.rs
- apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs
- apps/screenpipe-app-tauri/src-tauri/src/brain_runtime.rs
- apps/screenpipe-app-tauri/src-tauri/src/office_runtime.rs
- apps/screenpipe-app-tauri/src-tauri/assets/extensions/brain-tools.ts
- apps/screenpipe-app-tauri/components/brain/knowledge-hub.tsx
- apps/screenpipe-app-tauri/lib/utils/internal-session.ts
- apps/screenpipe-app-tauri/lib/stores/pi-event-router.ts
- apps/screenpipe-app-tauri/lib/stores/chat-store.ts
related:
- docs/roadmap.md
- docs/reviews/personal-brain-implementation-readiness.yaml
approval:
  basis: 用户授权实现审查及修复规划，并于2026-09-08确认将入口分工和统一任务机制纳入本次迭代基础改造。
  scope: 用户已授权按完整计划实施；本轮由当前会话直接编写、审查和验证，不启用子智能体。
implementation:
  status: in_progress
  task_id: 2026-09-08-local-brain-foundation-f02
  plan_to_build: required_per_batch
  completed_units:
  - F01
  current_unit: B01
  product_acceptance: not_run
  review_mode: final_batch_review
source_artifacts:
- name: personal-brain-local-first.md
  uri: docs/prd/personal-brain-local-first.md
  sha256: 8f23d662cd4ddeb274d61a20eb19eca5d69b0c737305778ddf3d9fa98d57df4d
- name: personal-brain-local-first.md
  uri: docs/trd/personal-brain-local-first.md
  sha256: 04444417f4ebefd13d54165dc7f59b7852ef72bb5579b86c112556b16264c3f4
- name: personal-brain-local-first-execution-plan.md
  uri: docs/plans/personal-brain-local-first-execution-plan.md
  sha256: 401cf55509363f0eb0da896fb75d6e8886f59575cb68dcbf93da32a41aae755c
- name: personal-brain-implementation-readiness.yaml
  uri: docs/reviews/personal-brain-implementation-readiness.yaml
  sha256: b23269082df96665f4c02065eda39109838432fb0a0c92d4e434a9a08038c043
- name: manifest.json
  uri: docs/reviews/evidence/personal-brain-implementation-review/manifest.json
  sha256: 52c3bdfb39b2141c6fcb3159101f536dd760ef5256aafad5807bb21bfe766cc3
- name: AGENTS.md
  uri: AGENTS.md
  sha256: 5e285bd461f88c7ad9e469f36addd9a9d33129b5b04f04cc1e91fd9e95f28542
- name: VISION.md
  uri: VISION.md
  sha256: bf2b38ef85a3642fb293d0bfc7b9960ced3e67ead8c7a9cad62986d30a42dac0
- name: mod.rs
  uri: crates/screenpipe-core/src/pipes/mod.rs
  sha256: e6ad6fbf3a42fce9ae9432aa27013f5954ac3f45a0dc1aea6fbc499d24b46746
- name: pipe_store.rs
  uri: crates/screenpipe-engine/src/pipe_store.rs
  sha256: a21310ec1e98116bea50ce9e54ea93ff4bcab073b473dbca660262b32076ef32
- name: worker.rs
  uri: crates/screenpipe-engine/src/brain/worker.rs
  sha256: 6a557218c93d6188ffa7f3d6cfa87439d079918f99cb4687bdf519b96abcf7d3
- name: activity_history.rs
  uri: apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs
  sha256: b18ae9345c2c406b13f6463ad748d73bb8ceb2789ba8b24c5506d5fc4c313296
- name: brain_runtime.rs
  uri: apps/screenpipe-app-tauri/src-tauri/src/brain_runtime.rs
  sha256: 954dfd8d197c6b5baec712ba28787ccc3587b5ce2d7a9a468ef3e7ec9cee32f5
- name: office_runtime.rs
  uri: apps/screenpipe-app-tauri/src-tauri/src/office_runtime.rs
  sha256: 2bb54ac46b8d707551c86d93b86c37533441ac041da68f6036eb5a87b4ff1677
- name: brain-tools.ts
  uri: apps/screenpipe-app-tauri/src-tauri/assets/extensions/brain-tools.ts
  sha256: 4cfe857a50b6ee9cd1039172dde20791b87939f43dbb5dd3c81a8c85be8106ec
- name: knowledge-hub.tsx
  uri: apps/screenpipe-app-tauri/components/brain/knowledge-hub.tsx
  sha256: 48e100ca569d8f7134192830f0b48a7f57165f4678d5fd012dc5a3343b1b283e
- name: internal-session.ts
  uri: apps/screenpipe-app-tauri/lib/utils/internal-session.ts
  sha256: 35e8fd5bcad91c1f3cf90fb9286e5a893c8e6a71d3960889d75d3103fb690517
- name: pi-event-router.ts
  uri: apps/screenpipe-app-tauri/lib/stores/pi-event-router.ts
  sha256: 6249753774efdf4c61eb445281bacf4b97d89573bdbb023efc82c9dfcec509fa
- name: chat-store.ts
  uri: apps/screenpipe-app-tauri/lib/stores/chat-store.ts
  sha256: 5cec3906c45a02f8ddb26776e97b7d40b0d506548c4a39055d944a1a0b5c52f8
source_hash: b65ab4381055f7e0d71af6761483909b7ca3f865f3bcd17b518bdf42ff989f0a
execution_control:
  coding_actor: current_session
  coding_model: null
  reasoning_effort: current_session
  reviewer: local_lead
  max_repair_rounds_per_issue: 3
  on_repair_exhausted: stop_and_request_user_direction
  commits: only_when_requested
  push: not_authorized
  review_mode: final_batch_review
  node_validation: implementer_self_checks
  progress_reporting: completion_or_blocker_only
  authorization_update: 2026-09-08 用户明确要求当前会话直接完成实现，不安排子智能体；整批统一验收，取消逐节点交回要求。
execution_batches:
- plan_unit_id: B01
  title: 完整基础改造实施与自测批次
  member_units:
  - F01
  - F02
  - F12
  - F03
  - F09
  - F04
  - F13
  - F05
  - F06
  - F07
  - F14
  - F08
  - F10
  - F11
  write_ownership: 上述member_units的write_ownership并集；Task Pack进一步列出精确并集及禁止写入的冻结文档。
  dependency_policy: 按既有DAG串行推进，节点自测通过后自动接续；跨节点联调缺口可在本批次内闭环，所有残留必须显式记录。
  review_policy: 主Agent仅在整批交付后集中审查；该包装不新增产品范围或第15个DAG节点。
---
# 知迹 · Local Brain 首版基础改造与正确性修复计划

<!-- doc-covers: none -->

> 原审查基线5fcacf6b1和计划ID保持稳定；本次范围修订已核对HEAD `0a83343ac7e75cc67effa3dae4430bcb4dd825cf`。在原正确性修复上增加R12/R13入口与任务基础改造，仍只接飞书/腾讯会议。共14节点、45项验收；approved表示用户已确认范围，代码实施已获授权，按节点开工检查后执行。旧37项实施审查及验证文件保持历史快照，不作为当前版本通过证明。

## 1. 为什么需要本次迭代

已有 DB、Worker、CLI 适配、知识审核、FTS、问答和 MCP 的基础实现。主要差距在跨模块的一致性：删除后的正文仍在、恢复不重放、发布版本身份混用、引用/过滤未生效、连接控制和审核闭环不完整。不能仅把剩余工作归结为安装 CLI、补权限或做 U12 样本验证。

审查结果见 [实施质量报告](../reviews/personal-brain-implementation-readiness.yaml)：37 项逐项映射，25 项存在实现不符合、5 项仅有部分证据、7 项正式评测未完成。这是验收状态，不是开发完成率。10 个合成契约反例均已复现，正常检索对照通过；[源码和结果](../reviews/evidence/personal-brain-implementation-review/)可直接作为回归起点。

本次新确认：工作单元在知识库展示，提问进入聊天，运行过程进入系统活动，自动化统一管理任务定义；连接继续作为办公配置唯一编辑入口。统一Pipes/活动总结/Brain/办公同步的任务生命周期与资源能力，业务处理器仍分别保留证据、权限和游标规则。

继续复用已落地模块，先修复会丢失、复活或误用资料的路径，再完成公共任务基础与可用闭环。无需重做规划前办公调研，也不要求先拿到真实授权样本才能开始隔离测试与实现修复。

## 2. 本次范围与完成标准

- 只接飞书消息/文档和腾讯会议转写/纪要，操作留在主侧栏「连接」。默认手动，自动同步显式启用，只读已选范围。
- 保留知迹品牌；技术 ID、包名、URL、协议、存储键和上游署名按约定保留。
- 按 2026-09-08 用户决定复用选中 AI 预设并允许手动切换 Runtime（含既有 ACP），不恢复固定 DeepSeek 限制，不新增费用或每日调用总量否决阈值。
- 来源支持、只检索本地、取消/删除清理和凭据仅 Keychain 引用仍须兑现。F01 明确各 Runtime 能实现的受限能力；能力不足返回具名不可用，不静默换模型，也不把选择 ACP 本身视为违规。
- 历史保全、首次启用后新增优先、显式 7 天回填、三种知识类型、自审发布、纠错和完整删除沿用原 PRD。WPS、向量、外部 MemoryProvider、国内 Runtime 新适配、全账号历史导入、办公写操作继续后置。
- 本次基础范围：知识库=工作单元/知识/画布；聊天=提问/引用/反馈；系统活动=按根run聚合的只读消息/状态；自动化=内置/用户任务定义；连接=办公账号/范围/同步的唯一配置源。移除知识库的Ask/status前先补齐聊天契约。
- 复用Pipes配置/触发与Brain持久任务，统一运行/尝试/事件/租约/预算；类型化处理器保留各自业务规则。前台聊天共享资源但不逐轮注册自动化，采集循环/UI定时器不迁入。
- 完成条件：本文14个节点证据齐备，原37项加R12/R13新增8项，共45项逐项通过或由用户明确接受具名风险；未测不能算通过。真实数据与秘密不进 git，最终仍以原 PRD §8 为质量阈值真源。

## 3. DAG、执行顺序与共用文件

```mermaid
flowchart LR
  F01[契约与回归基线]
  F02[删除与恢复]
  F12[公共任务存储与事件]
  F03[历史保全]
  F09[共享资源与Runtime]
  F04[抽取与编译]
  F13[Brain纵向接管]
  F05[版本与反馈]
  F06[检索与回答]
  F07[连接控制]
  F14[活动办公Pipes迁移]
  F08[办公证据链]
  F10[五入口与品牌]
  F11[45项验收]
  F01 --> F02
  F02 --> F12
  F02 --> F03
  F12 --> F09
  F03 --> F04
  F12 --> F04
  F09 --> F04
  F04 --> F13
  F09 --> F13
  F13 --> F05
  F05 --> F06
  F12 --> F07
  F03 --> F14
  F07 --> F14
  F13 --> F14
  F04 --> F08
  F07 --> F08
  F14 --> F08
  F05 --> F10
  F06 --> F10
  F07 --> F10
  F08 --> F10
  F09 --> F10
  F14 --> F10
  F10 --> F11
```

按依赖层数的最长链为`F01 → F02 → F12 → F09 → F04 → F13 → F14 → F08 → F10 → F11`；缺少工时估计，不用节点数推算交付日期。采用单writer串行顺序：`F01 → F02 → F12 → F03 → F09 → F04 → F13 → F05 → F06 → F07 → F14 → F08 → F10 → F11`。原F01–F11不重编号，新增F12–F14插入实际依赖位置；正文仍按稳定编号便于检索。

先验证F02删除反例、F12持久完成/事件一致性与F09资源等待，再通过F13/F14证明逐类型单所有者接管。界面依赖真实服务和迁移结果，不先聚合显示四套独立调度器。

DB schema、来源状态、任务、Brain REST、连接和 frontend DTO 存在直接/间接共享。路径归一化后确认有交集，因此选 `serial_shared_writer`；UI 尾段用 `serial_same_worktree`。不为表面并行拆出重复类型或多个 SQLite writer。

## 4. 公共节点契约

每节点继承下列字段，节点字段仅补充/收窄。完整文件 hash 在交接时计算，不写回自身，避免循环。`source_hash` 是 frontmatter.source_artifacts 的 UTF-8、键排序、紧凑 JSON SHA-256。未来开工先核对当前 HEAD/工作区漂移；不得回退到此基线覆盖其他开发。原审查末尾排除的pi.rs与brain-tools改动已提交到本次reviewed_head；本次读取确认桥接存在但trimAnswer丢失反馈身份。旧报告不追记通过，F01/F06/F09按当前实现修复。base_commit保留原计划身份，reviewed_head标明范围修订核对点；未来只向前吸收变更，不回退到旧提交。
```yaml
node_defaults:
  plan_id: plan-personal-brain-correctness
  source_plan_sha256: resolved_from_complete_plan_at_handoff
  base_commit: 5fcacf6b1a51bfaa0003e9e9907b608e80bad568
  task_id: null
  source_task_pack_sha256: null
  source_artifacts: inherit_frontmatter_source_artifacts
  source_hash: b65ab4381055f7e0d71af6761483909b7ca3f865f3bcd17b518bdf42ff989f0a
  actor: current_session
  review_actor: local_lead
  coding_model: null
  reasoning_effort: current_session
  parallel_mode: serial_shared_writer
  required_skills:
  - agent-brain
  - codebase-analysis
  required_capabilities:
  - Rust/SQLx/TypeScript 仓库读写
  - 隔离夹具与故障注入测试
  forbidden_writes:
  - 当前节点 write_ownership 之外的文件及无关现有改动
  - .git/**、真实用户 SQLite/store、全局 CLI 认证文件、真实办公/模型密钥、保留验收答案
  - 原始采集/编码热循环、WPS/新 Runtime/向量/外部 MemoryProvider 实现
  - 发布指针、发布标签、全局 MCP 覆盖、远程 push；没有单独授权不执行外发
  - 既有已部署 migration 内容；修正 schema 一律新增增量 migration
  evidence_required:
  - 节点 ID、实际 git_head、当前计划/source hash、精确 changed_files
  - 实际命令、exit_code、执行测试数、通过/失败/未测及原因；零测试匹配不算通过
  - 反例修复前后断言、持久状态和重启结果；mock/合成/真实证据明确区分
  - 由 Task Pack 指定 node-evidence/<unit-id>.json 路径；仅脱敏摘要/合成夹具进 git
  source_review: docs/reviews/personal-brain-implementation-readiness.yaml
```

SCOPE-R12/SCOPE-R13表示本次批准的新增范围，不是旧审查finding。以下各节点继承公共契约；节点状态与修正轮次记录在对应Task Pack和批次进度中，未通过审查/验证不得标完成。

同一节点需要修改额外公共文件、绑定或依赖时，先更新本计划所有权与 hash，再由单 writer 修改。已有反例源码在文档区，开工时转为正常回归测试；审查时故意保留的失败结果不是完成证据。

## 5. 节点执行契约

### F01 · 固定修复契约与可执行回归基线

```yaml
inherits: node_defaults
plan_unit_id: F01
depends_on: []
resolves:
- DR-LIFECYCLE-001
- DR-HISTORY-001
- DR-EXTRACT-001
- DR-KNOWLEDGE-001
- DR-RETRIEVAL-001
- DR-ANSWER-001
- DR-OFFICE-001
- DR-OFFICE-002
- DR-RUNTIME-001
- DR-REVIEW-001
- DR-BRAND-001
- DR-VERIFY-001
acceptance_ids:
- AC-R1-01
- AC-R1-02
- AC-R1-03
- AC-R1-04
- AC-R2-01
- AC-R2-02
- AC-R2-03
- AC-R3-01
- AC-R3-02
- AC-R3-03
- AC-R4-01
- AC-R5-01
- AC-R5-02
- AC-R5-03
- AC-R6-01
- AC-R6-02
- AC-R6-03
- AC-R6-04
- AC-R7-01
- AC-R7-02
- AC-R8-01
- AC-R8-02
- AC-R8-03
- AC-R9-01
- AC-R9-02
- AC-R9-03
- AC-R9-04
- AC-R11-01
- AC-R11-02
- AC-R11-03
- AC-EVAL-01
- AC-EVAL-02
- AC-EVAL-03
- AC-EVAL-04
- AC-EVAL-05
- AC-EVAL-06
- AC-EVAL-07
- AC-R12-01
- AC-R12-02
- AC-R12-03
- AC-R13-01
- AC-R13-02
- AC-R13-03
- AC-R13-04
- AC-R13-05
write_ownership:
- docs/trd/personal-brain-local-first.md
- docs/prd/personal-brain-local-first.md
- docs/plans/personal-brain-correctness-execution-plan.md
- crates/screenpipe-db/src/db/brain/types.rs
- crates/screenpipe-engine/src/brain/types.rs
- crates/screenpipe-connect/src/office/types.rs
- apps/screenpipe-app-tauri/lib/brain/types.ts
- apps/screenpipe-app-tauri/lib/connections/office-types.ts
- crates/screenpipe-engine/tests/brain_correctness.rs
- crates/screenpipe-db/tests/brain_correctness.rs
- crates/screenpipe-core/src/tasks/
- crates/screenpipe-db/src/db/tasks/
- crates/screenpipe-engine/src/tasks/
- apps/screenpipe-app-tauri/lib/tasks/
- crates/screenpipe-engine/tests/task_contracts.rs
- crates/screenpipe-db/tests/task_contracts.rs
mutex:
- brain-contracts
- dto
- test-fixtures
parallel_mode: serial_shared_writer
required_skills:
- agent-brain
- codebase-analysis
- write-trd
```

1. 核对 current_version_row_id 与 version、source/current revision、连接/范围 revision、删除 epoch 和任务提交条件；在 TRD 中明确数据关系。历史 plan 身份不重写。
2. 将 P01–P10 反例及 P11 对照转成隔离测试；按后续节点分组。补并发删除/提交、未启用/首次启用、迁移 coverage 和连接暂停的失败夹具。开发期允许这些断言先红，禁止通过弱化 AC 让它们变绿。
3. 按已更新TRD冻结TaskDefinition/Run/Attempt/Event、单所有者切换与各处理器重试策略；对既有Pipes配置/API/触发/权限/历史建立行为对照。选中Preset和Pi/ACP能力限制分别可测，不引入新Runtime适配；确定受管消息的保留上限与旧历史兼容方式。

验证检查点（新增测试过滤词由 F01/责任节点落实，必须匹配实际测试）：

- cargo test -p screenpipe-engine --test brain_correctness -- --nocapture（新增套件；输出逐例基线，预期仍有对应失败）
- cargo test -p screenpipe-db --test brain_correctness -- --nocapture（新增套件）
- 校验45项与F01–F14映射、DTO/错误码兼容及计划 hash。

交接信号：每个缺陷有准确失败断言或具名完整调用链；公共身份及提交条件定稿，后续无需猜测状态语义。

### F02 · 完成删除、恢复、retention 和受管正文清理

```yaml
inherits: node_defaults
plan_unit_id: F02
depends_on:
- F01
resolves:
- DR-LIFECYCLE-001
- DR-ANSWER-001
acceptance_ids:
- AC-R1-02
- AC-R1-03
- AC-R2-03
- AC-R3-02
- AC-R4-01
- AC-R5-03
- AC-R6-01
- AC-R6-02
- AC-R6-03
- AC-R7-02
- AC-R9-02
- AC-R9-03
- AC-R13-05
write_ownership:
- crates/screenpipe-db/src/db/brain/
- crates/screenpipe-db/src/db/maintenance.rs
- crates/screenpipe-db/src/db/activity_ledger.rs
- crates/screenpipe-db/src/db/memories.rs
- crates/screenpipe-db/src/migrations/*brain*.sql
- crates/screenpipe-engine/src/brain/deletion.rs
- crates/screenpipe-engine/src/brain/sources.rs
- crates/screenpipe-engine/src/brain/knowledge.rs
- crates/screenpipe-engine/src/routes/data.rs
- crates/screenpipe-engine/src/routes/memories.rs
- crates/screenpipe-engine/src/retention.rs
- crates/screenpipe-core/src/memories/external_sync.rs
- crates/screenpipe-engine/tests/brain_correctness.rs
- crates/screenpipe-db/tests/brain_correctness.rs
- crates/screenpipe-db/src/db/tasks/
- crates/screenpipe-engine/src/tasks/
- crates/screenpipe-db/src/migrations/*tasks*.sql
- crates/screenpipe-engine/src/brain/mod.rs
- crates/screenpipe-engine/src/server.rs
- apps/screenpipe-app-tauri/src-tauri/src/brain_runtime.rs
- crates/screenpipe-engine/src/brain/routes.rs
- crates/screenpipe-engine/src/brain/answer.rs
mutex:
- sqlite-schema
- db-writer
- deletion-barrier
parallel_mode: serial_shared_writer
```

1. 构建受协调 writer 管理的提交检查/删除事务：来源→Work Unit→全部知识版本→回答/claims/反馈/history→索引/受管文件/会话登记。显式删除擦除复制正文，保留无正文身份与墓碑；原始范围删除、memory、办公 erase 统一进入此链。
2. 必须覆盖Engine发布共享状态/HTTP路由和Tauri启动Worker的实际时序，恢复未完成时拒绝正文读取/模型取包；emission guard覆盖实际响应发送边界，不能只保护构造Response。journal fsync 后持久化一致序号；启动先比较 DB 与 journal，再提供读/写服务。恢复“journal 已写 DB 未提交、DB 提交文件未清、旧备份回退、并发删除、损坏尾行”；不得直接把未完成项置 completed。单文件失败保持待重试且可重启恢复。
3. 把task快照/消息payload、系统活动和聊天受管回答纳入依赖清理及读屏障；F12新增表必须使用这些接口，不另做best-effort清理。统一 office_object 和 source_locator 墓碑语义，保持账号隔离与重连抑制；旧数据修复不得重新暴露已删正文。
4. retention 删除前计算发布引用文本豁免，多知识共享引用最后释放才可清理；媒体可删除，归档引用有可核验摘录和真实状态。普通数据删除不得依赖原文删完后的事后补偿。

验证检查点（新增测试过滤词由 F01/责任节点落实，必须匹配实际测试）：

- cargo test -p screenpipe-engine --test brain_correctness deletion -- --nocapture
- cargo test -p screenpipe-db --test brain_correctness deletion -- --nocapture
- 若修改Tauri启动集成，在桌面目录运行 `bun run test:tauri brain_runtime -- --nocapture`；必须匹配实际测试，禁止裸Cargo或另设target。
- 临时目录注入 remove 失败、journal/DB 故障点和旧备份恢复；扫描原始/派生表、FTS 和受管文件，命中数必须为 0。
- 覆盖 frame/audio/ui_event/memory/office 五类；P01/P02/P09 通过，retention 发布豁免与共享释放符合 PRD 矩阵。

交接信号：删除成功确实没有受影响正文可读/可重建/可重导；文件失败不假完成；新持久正文只能走受保护存储接口。

### F03 · 历史迁移接管唯一读写来源

```yaml
inherits: node_defaults
plan_unit_id: F03
depends_on:
- F02
resolves:
- DR-HISTORY-001
acceptance_ids:
- AC-R1-04
- AC-R6-02
- AC-R8-03
write_ownership:
- apps/screenpipe-app-tauri/src-tauri/src/brain_migration.rs
- apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs
- crates/screenpipe-engine/src/brain/migration.rs
- crates/screenpipe-db/src/db/brain/history.rs
- crates/screenpipe-db/src/db/brain/state.rs
- crates/screenpipe-db/src/migrations/*brain*.sql
- crates/screenpipe-engine/tests/brain_correctness.rs
- crates/screenpipe-db/tests/brain_correctness.rs
mutex:
- history-writer
- desktop-lifecycle
- db-writer
parallel_mode: serial_shared_writer
```

1. 先暂停/排空旧 writer 再取稳定快照，逐项 ID+不改变字符串正文的摘要对照；coverage 逐区间、边界完整比较，不以条数下限替代。
2. 保持加密历史保护级别，Keychain 不可用暂停；验证后原子切换读写到 Brain，旧 store 只作为受控可清理回退副本。离线新条目必须有可重放的持久写入，禁止只有 best-effort 镜像。
3. 所有迁移/恢复/回退先应用 F02 删除记录；active、enabled_at 和保全迁移状态分离，不因导入旧记录而触发旧数据 AI 重算。

验证检查点（新增测试过滤词由 F01/责任节点落实，必须匹配实际测试）：

- cargo test -p screenpipe-engine --test brain_correctness migration -- --nocapture
- cargo test -p screenpipe-db --test brain_correctness migration -- --nocapture
- 桌面目录：bun run test:tauri brain_migration -- --nocapture（新增纯夹具测试，经队列/cache）；覆盖不同 entry/span 数、同数不同区间、正文空白、切换点失败、加密 key 不可用。

交接信号：新旧对照一致且 read_all/write_all 真正消费切换状态；断网/崩溃不丢条目，历史查看与 AI 启用边界分离。

### F04 · 修复增量任务、来源引用与三类知识编译

```yaml
inherits: node_defaults
plan_unit_id: F04
depends_on:
- F03
- F12
- F09
resolves:
- DR-EXTRACT-001
acceptance_ids:
- AC-R1-01
- AC-R1-02
- AC-R1-03
- AC-R1-04
- AC-R2-01
- AC-R6-02
- AC-R8-01
- AC-R8-02
- AC-R8-03
- AC-R9-03
- AC-EVAL-02
write_ownership:
- crates/screenpipe-engine/src/brain/extract.rs
- crates/screenpipe-engine/src/brain/compile.rs
- crates/screenpipe-engine/src/brain/sources.rs
- crates/screenpipe-engine/src/brain/registry/
- crates/screenpipe-engine/src/brain/prompts/
- crates/screenpipe-engine/src/brain/migration.rs
- crates/screenpipe-db/src/db/brain/work_units.rs
- crates/screenpipe-db/src/db/brain/jobs.rs
- crates/screenpipe-db/src/db/brain/knowledge.rs
- crates/screenpipe-db/src/db/brain/state.rs
- crates/screenpipe-db/src/migrations/*brain*.sql
- apps/screenpipe-app-tauri/src-tauri/src/brain_runtime.rs
- crates/screenpipe-engine/tests/brain_correctness.rs
- crates/screenpipe-engine/src/tasks/
- crates/screenpipe-db/src/db/tasks/
mutex:
- db-writer
- brain-jobs
- knowledge-registry
- desktop-lifecycle
parallel_mode: serial_shared_writer
```

1. 由公共任务处理器按enabled/enabled_at、final稳定区间和持久checkpoint发现新增，不新增模块定时器；任务身份含区间、来源修订、人工分类和抽取器/模型身份。同 scope 不同会话不能被合并，同输入重启不重复模型调用；停机超过 26 小时仍可续接。
2. 修复 ui_events/AX 来源表映射和实际时间/应用元数据；eN/uN 字段引用映射与精确 revision 随正文保存；使用 F02 的原子提交接口把产物、依赖和 job 完成一起提交。模型返回后的 cancel/lease/revision/deletion 不通过则全部丢弃。
3. 显式回填只覆盖所选近 7 天，跨界区间规则可核验；稳定流程 scope 与每个会话身份分离。单次 DecisionRule/ExceptionPlaybook 允许带边界产生；SOP 的三个独立会话和步骤/条件/异常依据从输入计算。
4. 输入 hash 使用 Work Unit 当前修订及真实 prompt/schema/model 身份；编译前消费驳回抑制，重复输入不再新增候选；拒绝空步骤及没有支持的事实字段。

验证检查点（新增测试过滤词由 F01/责任节点落实，必须匹配实际测试）：

- cargo test -p screenpipe-engine --test brain_correctness extraction -- --nocapture
- cargo test -p screenpipe-db --test brain_correctness jobs -- --nocapture
- 覆盖启用前/跨界/迟到/分拆合并/分类变化、同标题不同区间、停机 48 小时、模型返回瞬间删除/失租约；P10 通过。
- 同输入重跑/驳回无新增，修订输入有新候选；三个独立会话与单次规则各有成功对照。

交接信号：范围、进度、产物一致，取消或删除无旧正文提交；引用能跨重启还原，三类候选门槛分别成立。

### F05 · 修复知识版本、原子审核与反馈定位服务

```yaml
inherits: node_defaults
plan_unit_id: F05
depends_on:
- F13
resolves:
- DR-KNOWLEDGE-001
- DR-REVIEW-001
acceptance_ids:
- AC-R2-02
- AC-R2-03
- AC-R5-03
- AC-R7-01
- AC-R7-02
- AC-R12-01
write_ownership:
- crates/screenpipe-db/src/db/brain/knowledge.rs
- crates/screenpipe-db/src/db/brain/work_units.rs
- crates/screenpipe-db/src/db/brain/feedback.rs
- crates/screenpipe-db/src/migrations/*brain*.sql
- crates/screenpipe-engine/src/brain/knowledge.rs
- crates/screenpipe-engine/src/brain/routes.rs
- crates/screenpipe-engine/src/brain/types.rs
- apps/screenpipe-app-tauri/lib/brain/types.ts
- crates/screenpipe-engine/tests/brain_correctness.rs
- crates/screenpipe-db/tests/brain_correctness.rs
mutex:
- sqlite-schema
- db-writer
- knowledge-state
- brain-router
- dto
parallel_mode: serial_shared_writer
```

1. 统一知识 row ID/版本号并对已有错误指针做增量修复；发布候选、旧版 supersede、指针/epoch 更新在一事务完成，任何 CAS 失败完全回滚。
2. 候选编辑正文与 revision 同事务；发布时按准确依赖修订、有效性、归档可核验性和 F02 删除条件重验。新事实必须有支持，不能只检查引用 ID 存在。
3. 增加工作单元列表/详情及知识→支持工作单元/会话反查契约，包含字段证据、原始活动、相关知识和有效修订；删除/过期仍走读屏障，未知项目不补造。提供从发布版创建候选修订、暂停/恢复/驳回原因和 30 天复核的明确 API；结构化定位 answer/claim→知识版本，未知关联进入可管理待处理队列；删除时反馈和历史正文服从 F02。

验证检查点（新增测试过滤词由 F01/责任节点落实，必须匹配实际测试）：

- cargo test -p screenpipe-db --test brain_correctness publication -- --nocapture
- cargo test -p screenpipe-engine --test brain_correctness review -- --nocapture
- P07/P08 必须通过；至少两条知识、三轮版本、并发编辑/发布和失败回滚；发布瞬间删来源不得成功。
- 证明 v1 已发布/v2 待审期间 v1 可用，v2 发布后只引用 v2；到期/暂停的服务端语义明确。

交接信号：版本身份不再歧义，失败操作不伤当前版；反馈、修订、自审服务足以支持完整 UI 闭环。

### F06 · 检索、回答和 MCP 共用当前证据规则

```yaml
inherits: node_defaults
plan_unit_id: F06
depends_on:
- F05
resolves:
- DR-RETRIEVAL-001
- DR-ANSWER-001
- DR-VERIFY-001
acceptance_ids:
- AC-R3-01
- AC-R3-02
- AC-R3-03
- AC-R4-01
- AC-R5-01
- AC-R5-02
- AC-R5-03
- AC-R7-01
- AC-EVAL-03
- AC-R12-02
write_ownership:
- crates/screenpipe-engine/src/brain/search.rs
- crates/screenpipe-engine/src/brain/answer.rs
- crates/screenpipe-engine/src/brain/routes.rs
- crates/screenpipe-engine/src/brain/prompts/
- crates/screenpipe-db/src/db/brain/search.rs
- crates/screenpipe-db/src/db/brain/feedback.rs
- crates/screenpipe-db/src/db/brain/sources.rs
- crates/screenpipe-db/src/text_normalizer.rs
- crates/screenpipe-db/src/migrations/*brain*.sql
- packages/screenpipe-mcp/src/
- packages/screenpipe-mcp/tests/
- packages/screenpipe-mcp/README.md
- crates/screenpipe-engine/tests/brain_correctness.rs
- apps/screenpipe-app-tauri/src-tauri/assets/extensions/brain-tools.ts
- apps/screenpipe-app-tauri/src-tauri/assets/extensions/brain-tools.test.ts
- crates/screenpipe-engine/src/tasks/
- crates/screenpipe-db/src/db/tasks/
mutex:
- search-contracts
- brain-router
- mcp-contracts
- db-writer
parallel_mode: serial_shared_writer
```

1. 来源、memory、知识三路统一当前修订/状态、应用/时间/来源种类过滤；到期、暂停、disabled、已删、切版均及时失效。对原始采集与办公事件分别使用真实发生时间，不用抓取时间补未知。
2. 明确 disabled/no_hits/timeout/failed/ok 与部分/全部失败；FTS 故障不得吞成零命中。保持旧 /search 的默认排序、升序、分页、过滤和空结果契约。
3. 拒绝空/缺失/伪造/不支持结论的引用；仅从已核验 claims 生成可见 answer，失效声明不能放到 uncertainty 泄漏。返回/保存前再验 source revision 与知识 publication epoch，memory 引用也可通过统一来源接口核验。
4. 幂等键绑定问题、filters、运行身份与有效性 epoch，缓存返回完整相同结果；正文、依赖、TTL 和保存状态通过 DatabaseManager 原子操作并实际回收。
5. 内部聊天桥接与REST/MCP保留同一结构化回答身份；修正brain-tools trimAnswer丢弃answer_id/claim_id/version/来源修订，过滤与范围完整传递，只有已校验声明可标有据。REST/MCP 同错误及引用规则；为 answer/get-brain-source 新工具补调用、删除、版本变更和包装分发测试，提供可复用连接说明，不能只留下某台机器的绝对路径手工注册。

验证检查点（新增测试过滤词由 F01/责任节点落实，必须匹配实际测试）：

- cargo test -p screenpipe-engine --test brain_correctness retrieval -- --nocapture
- cargo test -p screenpipe-engine --test brain_correctness answer -- --nocapture
- cargo test -p screenpipe-db --test brain_correctness search_compatibility -- --nocapture（新增旧契约夹具）
- MCP 目录：bun run test；bun run typecheck；新工具用例必须被实际选中。
- P03–P06/P11 全绿；伪造、真实但不支持、无证据、冲突、过期、注入、生成中删除/切版、同 key 不同问题和缓存过期有成功/失败对照。

交接信号：每个可见事实有当前可核验引用，检索失败与无证据明确区分；保存、缓存、REST 和 MCP 一致。

### F07 · 连接中的依赖/授权和同步控制闭环

```yaml
inherits: node_defaults
plan_unit_id: F07
depends_on:
- F12
resolves:
- DR-OFFICE-001
acceptance_ids:
- AC-R6-01
- AC-R6-04
- AC-R9-01
- AC-R9-02
- AC-R9-04
- AC-R13-01
- AC-R13-02
write_ownership:
- crates/screenpipe-connect/src/office/runner.rs
- crates/screenpipe-connect/src/office/types.rs
- crates/screenpipe-connect/src/office/feishu.rs
- crates/screenpipe-connect/src/office/tencent_meeting.rs
- crates/screenpipe-engine/src/brain/office.rs
- crates/screenpipe-engine/src/brain/office_routes.rs
- crates/screenpipe-db/src/db/brain/office.rs
- crates/screenpipe-db/src/db/brain/jobs.rs
- crates/screenpipe-db/src/migrations/*brain*.sql
- apps/screenpipe-app-tauri/src-tauri/src/office_runtime.rs
- apps/screenpipe-app-tauri/lib/connections/office.ts
- apps/screenpipe-app-tauri/lib/connections/office-types.ts
- crates/screenpipe-engine/tests/brain_correctness.rs
- crates/screenpipe-connect/tests/office_lifecycle.rs
- crates/screenpipe-engine/src/tasks/
- crates/screenpipe-db/src/db/tasks/
mutex:
- office-contracts
- db-writer
- office-router
- desktop-lifecycle
- dto
parallel_mode: serial_shared_writer
```

1. 复用已支持的官方 CLI 探测/安装机制和用户发起的正式登录路径；缺依赖/缺 scope/账号不支持分别呈现。认证流程真实启动并能轮询完成，不能以 refresh 冒充 authorize。使用专用受限动作，不开放任意代理。
2. 控制粒度固定provider+account+run；定义启停影响未来，cancel只作用选定run，断开/缩范围立即禁止旧结果提交。公共TaskService持久化暂停/恢复/重试/断开状态，重启继续有效，暂停飞书不得取消腾讯会议。自动同步使用持久游标和推进窗口，不能反复同步固定旧窗口。
3. 使用聊天→消息、文档、会议→录制资源关系计算范围；停用传播到来源、知识和检索。断开 retain_inactive 使用真实账号，erase 进入 F02；不注销/升级用户全局 CLI 登录。
4. 每页提交重验当前账号、connection/scope revision、取消/删除 epoch；对象、索引、映射、游标同事务或可恢复一致操作，范围变更后旧页不能落库。

验证检查点（新增测试过滤词由 F01/责任节点落实，必须匹配实际测试）：

- cargo test -p screenpipe-connect --test office_lifecycle -- --nocapture（新增 fake CLI 集成套件）
- cargo test -p screenpipe-engine --test brain_correctness office_control -- --nocapture
- 覆盖依赖缺失/授权失效/切号/限流/离线/分页中断、取消同时返回、缩范围、断开重连、两个 provider 同时有任务；测试真实状态而非仅按钮。

交接信号：全部连接操作有真实服务实现且可跨页/重启保留；没有范围外提交、跨 provider 误取消或删除复活。

### F08 · 办公正文、时间回链与活动关联

```yaml
inherits: node_defaults
plan_unit_id: F08
depends_on:
- F04
- F07
- F14
resolves:
- DR-OFFICE-002
acceptance_ids:
- AC-R1-01
- AC-R2-01
- AC-R9-01
- AC-R9-02
- AC-R9-03
- AC-EVAL-02
- AC-EVAL-07
- AC-R12-01
write_ownership:
- crates/screenpipe-connect/src/office/feishu.rs
- crates/screenpipe-connect/src/office/tencent_meeting.rs
- crates/screenpipe-connect/src/office/types.rs
- crates/screenpipe-connect/tests/fixtures/office/
- crates/screenpipe-connect/tests/office_content.rs
- crates/screenpipe-engine/src/brain/office.rs
- crates/screenpipe-engine/src/brain/extract.rs
- crates/screenpipe-engine/src/brain/sources.rs
- crates/screenpipe-db/src/db/brain/office.rs
- crates/screenpipe-db/src/db/brain/sources.rs
- crates/screenpipe-db/src/db/brain/search.rs
- crates/screenpipe-db/src/migrations/*brain*.sql
- crates/screenpipe-engine/tests/brain_correctness.rs
mutex:
- office-contracts
- knowledge-registry
- search-contracts
- db-writer
parallel_mode: serial_shared_writer
```

1. 保存真实会议/录制/段落身份、事件时间、段落时间锚和可安全打开的来源链接；未知保持未知，不能把 scope 起始时间当会议时间。原始转写与平台 AI 纪要分源。
2. 飞书文档/消息和会议段落采用有界可核验正文块，长文后段命中能查看对应片段；重建和增量索引保持同样覆盖与删除语义，不以 2000 字摘录重建替代全文索引。
3. 只有明确会议/消息/文档活动锚匹配到 final 会话的办公来源才进入 Work Unit；可访问但未使用的资料留作检索。复用 F04 稳定身份串联跨工具重复流程。

验证检查点（新增测试过滤词由 F01/责任节点落实，必须匹配实际测试）：

- cargo test -p screenpipe-connect --test office_content -- --nocapture（新增脱敏分页/长文/时间夹具）
- cargo test -p screenpipe-engine --test brain_correctness office_evidence -- --nocapture
- 合成跨工具流程完成来源→Work Unit→SOP 草稿→发布→问答→删除；活动/仅资料正反对照，长文后段重建前后检索一致。
- 真实两工具版本/权限及转写时间回链留 F11 验收；没有云转写时记不可用，不用截图或 AI 纪要冒充。

交接信号：办公内容既可检索也可按确凿活动证据参与沉淀，来源时间/身份/回链逐项可核验。

### F09 · Runtime 持久预算、凭据与取消清理

```yaml
inherits: node_defaults
plan_unit_id: F09
depends_on:
- F12
resolves:
- DR-RUNTIME-001
acceptance_ids:
- AC-R6-04
- AC-R8-01
- AC-R8-02
- AC-EVAL-06
- AC-R13-03
- AC-R13-05
write_ownership:
- crates/screenpipe-engine/src/brain/executor.rs
- crates/screenpipe-engine/src/brain/worker.rs
- crates/screenpipe-engine/src/brain/answer.rs
- crates/screenpipe-db/src/db/brain/jobs.rs
- crates/screenpipe-db/src/db/brain/state.rs
- crates/screenpipe-db/src/migrations/*brain*.sql
- apps/screenpipe-app-tauri/src-tauri/src/brain_runtime.rs
- apps/screenpipe-app-tauri/src-tauri/src/pi.rs
- apps/screenpipe-app-tauri/src-tauri/src/store.rs
- apps/screenpipe-app-tauri/src-tauri/src/main.rs
- crates/screenpipe-engine/tests/brain_correctness.rs
- crates/screenpipe-core/src/tasks/
- crates/screenpipe-db/src/db/tasks/
- crates/screenpipe-engine/src/tasks/
- crates/screenpipe-core/src/agents/
- crates/screenpipe-core/src/pipes/mod.rs
- crates/screenpipe-db/src/migrations/*tasks*.sql
- crates/screenpipe-engine/tests/task_contracts.rs
mutex:
- pi-executor
- desktop-lifecycle
- db-writer
- model-secrets
parallel_mode: serial_shared_writer
```

1. 沿用当前 preset/Runtime 选择，执行时冻结真实模型、provider、Pi/Runtime 版本和 profile；凭据由 Keychain 引用解析，不把值持久在新增 Brain 状态/日志/会话。需要修正已有 preset 存储时限定相关键的兼容迁移，保留其他设置。
2. 使用F12公共attempt/资源接纳接口，删除Brain、answer、Pipes各自绕过公共限制的模型许可路径；前后台模型调用总并发1、办公I/O按TRD分池。任务模型调用数跨进程/重试累计，非法 JSON 重试计入最多三次；未配置每日预算保持无限制。配置错误转可恢复暂停，修复预设后继续；超时、排队、输入上下文按 TRD 限制执行。
3. 交互与后台共享明确的资源限制/公平调度，长任务不能无限阻塞聊天中的Brain问答；请求取消、删除、失租约和退出统一传播，使用可在 future drop/启动失败后清理的进程管理与目录守卫。
4. 验证聊天tool等待期间释放模型许可，嵌套brain_answer不能等待被父会话占住的唯一许可，父deadline/预算不重置。启动恢复先清理旧受管会话再运行任务；取消需 kill/reap（终止并回收子进程）以及持久清理重试，不能只取消 Rust future 后留下 CLI。Pi/ACP 分别验证，能力不足报明确错误。

验证检查点（新增测试过滤词由 F01/责任节点落实，必须匹配实际测试）：

- cargo test -p screenpipe-engine --test brain_correctness runtime -- --nocapture
- 桌面目录：bun run test:tauri brain_runtime -- --nocapture（新增 fake sidecar/Keychain 测试，必须走 native queue/cache）
- 故障点包括启动失败、排队超时、执行超时、取消、退出重启、断网恢复、租约失效、模型切换；跨三轮重试累计调用不得超过三次。
- 检查受管文件/子进程和日志，不打印密钥；真实所选模型调用与资源指标在 F11 有条件实测，不能用 mock 替代。

交接信号：选中 Runtime 的实际执行身份可审计；取消/退出无旧结果或受管正文残留；资源预算与暂停/恢复状态跨重启一致。

### F10 · 工作单元在知识库展示、聊天问答、系统活动与自动化界面

```yaml
inherits: node_defaults
plan_unit_id: F10
depends_on:
- F05
- F06
- F07
- F08
- F09
- F14
resolves:
- DR-REVIEW-001
- DR-BRAND-001
acceptance_ids:
- AC-R2-02
- AC-R2-03
- AC-R5-01
- AC-R7-01
- AC-R7-02
- AC-R8-03
- AC-R9-04
- AC-R11-01
- AC-R11-02
- AC-R11-03
- AC-EVAL-05
- AC-R12-01
- AC-R12-02
- AC-R12-03
- AC-R13-01
- AC-R13-02
- AC-R13-05
write_ownership:
- apps/screenpipe-app-tauri/components/brain/
- apps/screenpipe-app-tauri/lib/brain/
- apps/screenpipe-app-tauri/components/settings/office-connection-card.tsx
- apps/screenpipe-app-tauri/components/settings/feishu-connection-panel.tsx
- apps/screenpipe-app-tauri/components/settings/tencent-meeting-connection-panel.tsx
- apps/screenpipe-app-tauri/components/settings/connections-section.tsx
- apps/screenpipe-app-tauri/components/settings/brain-section.tsx
- apps/screenpipe-app-tauri/components/settings/__tests__/
- apps/screenpipe-app-tauri/lib/brand.ts
- apps/screenpipe-app-tauri/lib/connections/
- crates/screenpipe-engine/src/brain/routes.rs
- docs/reviews/personal-brain-brand-retained-identifiers.md
- apps/screenpipe-app-tauri/app/(main)/home/page.tsx
- apps/screenpipe-app-tauri/components/chat/
- apps/screenpipe-app-tauri/components/chat-sidebar.tsx
- apps/screenpipe-app-tauri/components/pipe-store.tsx
- apps/screenpipe-app-tauri/components/settings/pipes-section.tsx
- apps/screenpipe-app-tauri/components/__tests__/local-brain-foundation.test.tsx
- apps/screenpipe-app-tauri/lib/tasks/
- apps/screenpipe-app-tauri/lib/stores/chat-store.ts
- apps/screenpipe-app-tauri/lib/stores/pi-event-router.ts
- apps/screenpipe-app-tauri/lib/utils/internal-session.ts
- apps/screenpipe-app-tauri/lib/stores/__tests__/task-events.test.ts
- crates/screenpipe-engine/src/brain/types.rs
- crates/screenpipe-db/src/db/brain/work_units.rs
- crates/screenpipe-engine/src/tasks/
mutex:
- brain-ui
- connections-ui
- frontend-brand
- brain-router
parallel_mode: serial_same_worktree
required_capabilities:
- React/TypeScript
- 本机浏览器操作与无障碍行为测试
```

1. 编辑状态绑定具体 candidate row/version/revision，不能借用当前发布版；实现从发布版创建 v2、来源对照、自审、驳回/恢复原因、到期复核和待处理反馈。
2. 主侧栏入口及关联标题、导航提示、无障碍名称统一为“知识库”。知识库固定工作单元/知识/画布：新增工作单元列表/详情和双向知识关系，不增逐WU审核。将提问迁入正常聊天，复用结构化结果卡和F06稳定ID、来源/版本/时间锚、过滤及反馈；从知识库携带引用发起提问。删除/切版及时清除受管历史和当前结果，闭环完成后移除知识库Ask/status。
3. 系统活动用统一root run聚合只读聊天消息，尝试/校验/重试/回填步骤在详情，按seq补拉去重；I/O显示事件，状态以DB成功提交为准。自动化区分内置/用户/连接任务，内置字段可编辑、校验不可改，运行跳转系统活动；定义启停与单次取消分开。连接保持账号/范围/自动同步唯一编辑真源。回填控制在自动化，运行进度在系统活动，服务返回真实coverage/积压/清理失败。
4. 修复 Screenpipe 产物预填文案及过期断言，按品牌常量和具名保留清单核对可见路径。先定位精确品牌残留再扩充当前允许文件，禁止机械全仓替换或批量更新快照。

验证检查点（新增测试过滤词由 F01/责任节点落实，必须匹配实际测试）：

- 桌面目录：bun x vitest run --config vitest.config.ts components/brain components/settings/__tests__/office-connection-card.test.tsx components/settings/__tests__/brain-section.filter.test.tsx components/settings/__tests__/brain-overview.test.tsx（新测试落点以此冻结）
- 桌面目录：bun run typecheck；browser-mock 完整执行“有误反馈→暂停 v1→编辑 v2→自审→再次提问只用 v2”。
- 桌面目录：bun x vitest run --config vitest.config.ts components/__tests__/local-brain-foundation.test.tsx lib/stores/__tests__/task-events.test.ts（新增行为套件；选择器必须匹配实际测试）。
- browser-mock完整走五入口；验证单run多attempt/重启补拉无重复、只读查看不重启任务、后台失败不假成功、纯I/O事件、定义启停与运行取消、连接配置跳转、删除清空消息、WU未知/过期/删除及画布兼容；检查键盘/焦点与可访问名称。
- 品牌扫描+具名例外+主要页面画面证据；83/1 的旧前端测试失败应精确修正，新增 UI 核心行为也必须有测试。

交接信号：用户能不依赖 SQL/命令行完成连接、审核、纠错与回填控制；界面所示状态等于实际服务状态。

### F11 · 45项完整回归与真实工作流验收

```yaml
inherits: node_defaults
plan_unit_id: F11
depends_on:
- F10
resolves:
- DR-VERIFY-001
acceptance_ids:
- AC-R1-01
- AC-R1-02
- AC-R1-03
- AC-R1-04
- AC-R2-01
- AC-R2-02
- AC-R2-03
- AC-R3-01
- AC-R3-02
- AC-R3-03
- AC-R4-01
- AC-R5-01
- AC-R5-02
- AC-R5-03
- AC-R6-01
- AC-R6-02
- AC-R6-03
- AC-R6-04
- AC-R7-01
- AC-R7-02
- AC-R8-01
- AC-R8-02
- AC-R8-03
- AC-R9-01
- AC-R9-02
- AC-R9-03
- AC-R9-04
- AC-R11-01
- AC-R11-02
- AC-R11-03
- AC-EVAL-01
- AC-EVAL-02
- AC-EVAL-03
- AC-EVAL-04
- AC-EVAL-05
- AC-EVAL-06
- AC-EVAL-07
- AC-R12-01
- AC-R12-02
- AC-R12-03
- AC-R13-01
- AC-R13-02
- AC-R13-03
- AC-R13-04
- AC-R13-05
write_ownership:
- docs/reviews/personal-brain-correctness-acceptance.md
- docs/reviews/personal-brain-correctness-readiness.yaml
- docs/reviews/evidence/personal-brain-correctness/
- docs/reviews/personal-brain-foundation-plan-validation.json
- docs/roadmap.md
- docs/plans/personal-brain-correctness-execution-plan.md
- apps/screenpipe-app-tauri/e2e/specs/local-brain*.spec.ts
- apps/screenpipe-app-tauri/src-tauri/src/e2e/commands.rs
- apps/screenpipe-app-tauri/src-tauri/gen/schemas/
- crates/screenpipe-engine/tests/brain_correctness.rs
- crates/screenpipe-db/tests/brain_correctness.rs
- crates/screenpipe-connect/tests/office_lifecycle.rs
- crates/screenpipe-connect/tests/office_content.rs
- packages/screenpipe-mcp/tests/
- crates/screenpipe-engine/tests/task_contracts.rs
- crates/screenpipe-db/tests/task_contracts.rs
- crates/screenpipe-engine/tests/task_migration.rs
mutex:
- acceptance-snapshot
- release-measurement
parallel_mode: serial_same_worktree
required_skills:
- agent-brain
- delivery-readiness
- screenpipe-api
- screenpipe-cli
required_capabilities:
- 本机桌面与浏览器
- 真实授权样本的本地隔离处理
- 人工事实/可用性评测
```

1. 先冻结修复候选 HEAD、完整源码/计划 hash 和各节点证据，运行合成全链及兼容测试；保留本轮原始审查，不覆盖成“当时已通过”。
2. 冻结 ≥10 真实开发会话（≥3 同流程）、30–50 保留查询、两款 CLI/账号能力和实际所选模型/Runtime；调试集与保留集分开，失败项计入分母，真实材料仅放本地受控目录。
3. 完成需求分析/调研/业务沟通中的至少一条跨工具链：连接导入→系统活动运行→知识库中工作单元/活动关联→SOP v1→聊天纠错v2→再次问答→来源删除；两工具关键内容正确导入且可检索分别 ≥90%，腾讯会议含真实转写和时间回链。
4. 按 PRD §8 记录事实支持 ≥90%、至少一份用户认可 SOP、保留集无无支持事实、可回答题正确实质回答 ≥80%，以及原资源/SLA、5 工作日审核负担和出口。无真实样本的条目记未测，不用全拒答/全抑制候选达标。
5. 验证R12/R13五入口及四套机制迁移、满载资源、所有权切换、定义/运行控制、运行消息删除与旧Pipes兼容。实际检查 REST/MCP 新工具分发、错误、修订/删除，旧 /search、memory、文件出口和品牌回归。缺陷回归所属节点修复，不在验收节点扩大代码所有权；全部关闭后再改 completed/roadmap。

验证检查点（新增测试过滤词由 F01/责任节点落实，必须匹配实际测试）：

- 根目录：cargo test -p screenpipe-db --lib brain；cargo test -p screenpipe-engine --lib brain；cargo test -p screenpipe-connect --lib office
- 根目录：cargo test -p screenpipe-db --test brain_correctness；cargo test -p screenpipe-engine --test brain_correctness；cargo test -p screenpipe-connect --test office_lifecycle --test office_content
- MCP 目录：bun run test；bun run typecheck；桌面目录相关 Vitest 与 bun run typecheck；原生边界只用 bun run test:tauri。
- 根目录：cargo test -p screenpipe-db --test task_contracts；cargo test -p screenpipe-engine --test task_contracts --test task_migration。
- 当前PRD45项逐项证据表：passed/failed/unverified/accepted_risk，accepted_risk 必须记用户、理由、范围及复查条件；没有新修改/失败不反复扩大测试。

交接信号：45项有真实证据或用户具名接受的风险，implementation_to_verify 再评估可过；未满足则保持进行中，不发布、不推送。

### F12 · 公共任务存储、定义、运行与受管事件

```yaml
inherits: node_defaults
plan_unit_id: F12
depends_on:
- F02
resolves:
- SCOPE-R12
- SCOPE-R13
acceptance_ids:
- AC-R12-03
- AC-R13-01
- AC-R13-02
- AC-R13-05
write_ownership:
- crates/screenpipe-core/src/tasks/
- crates/screenpipe-db/src/db/tasks/
- crates/screenpipe-engine/src/tasks/
- crates/screenpipe-db/Cargo.toml
- Cargo.lock
- apps/screenpipe-app-tauri/src-tauri/Cargo.lock
- crates/screenpipe-core/src/lib.rs
- crates/screenpipe-db/src/db/mod.rs
- crates/screenpipe-db/src/lib.rs
- crates/screenpipe-db/tests/sqlite_architecture_invariants_test.rs
- crates/screenpipe-engine/src/lib.rs
- crates/screenpipe-db/src/migrations/*tasks*.sql
- crates/screenpipe-core/src/pipes/mod.rs
- crates/screenpipe-engine/src/pipe_store.rs
- crates/screenpipe-db/src/db/brain/jobs.rs
- crates/screenpipe-engine/src/brain/deletion.rs
- crates/screenpipe-engine/tests/task_contracts.rs
- crates/screenpipe-db/tests/task_contracts.rs
- apps/screenpipe-app-tauri/lib/tasks/
- crates/screenpipe-engine/src/server.rs
mutex:
- task-contracts
- sqlite-schema
- db-writer
- deletion-barrier
parallel_mode: serial_shared_writer
required_skills:
- agent-brain
- codebase-analysis
- refactor-plan
```

1. 在既有crate中增加TRD §4.4公共类型/trait/存储/服务，不新增外部基础设施。定义、run、attempt、event、legacy映射与owner_generation走DatabaseManager；共用已修好的F02删除/读屏障，不能先落正文再补依赖。
2. 提供类型化内置定义和用户Pipe/连接配置引用。内置允许字段及校验不可编辑边界固定，办公更新只能回到连接真源；pipe.md保持用户配置正文真源，运行前冻结规范化revision/snapshot，禁止并行可编辑副本。
3. 统一领取/心跳/提交/失败/超时/暂停/取消及CAS（带预期版本才能更新）的接口；成功产物/依赖/cursor/run终态/完成事件同事务，消息按run+seq可重放去重。事件载荷登记受管来源依赖；外部副作用只记录回执或needs_attention，不承诺外部恰好一次执行。
4. 新服务只在测试/未接管类型的适配路径可用，不启用第二套生产触发器。冻结运行消息条数/体积/保留策略，保留旧合法历史，规定清理失败/失效消息返回形态。

验证检查点（新增套件，必须实际匹配测试）：

- cargo test -p screenpipe-db --test task_contracts -- --nocapture
- cargo test -p screenpipe-engine --test task_contracts lifecycle -- --nocapture
- 隔离库覆盖重复触发、租约过期、CAS冲突、每个提交故障点、完成事件丢通知后补拉、消息中途删除/旧备份重放、定义配置变更与连接禁止旁路编辑。

交接信号：公共类型/持久存储可供所有处理器复用，消息和产物都受删除保护；没有提前启动双调度。新增运行API的错误/状态映射及保留参数写回TRD，旧ID可稳定映射。

### F13 · Brain抽取、编译与回填纵向接管

```yaml
inherits: node_defaults
plan_unit_id: F13
depends_on:
- F04
- F09
resolves:
- SCOPE-R12
- SCOPE-R13
acceptance_ids:
- AC-R1-02
- AC-R8-01
- AC-R8-02
- AC-R8-03
- AC-R12-03
- AC-R13-02
- AC-R13-04
- AC-R13-05
write_ownership:
- crates/screenpipe-core/src/tasks/
- crates/screenpipe-db/src/db/tasks/
- crates/screenpipe-engine/src/tasks/
- crates/screenpipe-db/src/db/brain/jobs.rs
- crates/screenpipe-db/src/db/brain/state.rs
- crates/screenpipe-db/src/migrations/*tasks*.sql
- crates/screenpipe-engine/src/brain/worker.rs
- crates/screenpipe-engine/src/brain/extract.rs
- crates/screenpipe-engine/src/brain/compile.rs
- crates/screenpipe-engine/src/brain/routes.rs
- apps/screenpipe-app-tauri/src-tauri/src/brain_runtime.rs
- apps/screenpipe-app-tauri/src-tauri/src/main.rs
- crates/screenpipe-engine/tests/task_migration.rs
- crates/screenpipe-engine/tests/brain_correctness.rs
- crates/screenpipe-engine/src/server.rs
mutex:
- task-contracts
- brain-jobs
- db-writer
- desktop-lifecycle
parallel_mode: serial_shared_writer
required_skills:
- agent-brain
- codebase-analysis
- refactor-plan
```

1. 将已修正F04的抽取、编译、增量发现和回填注册为类型化任务；启用/范围/频率/预设通过公共目录管理，handler保留证据包、各类型门槛、来源/取消/删除校验，不把业务塞进可随意编辑的pipe.md。
2. 将旧brain_jobs、预算、进度和有效在途身份幂等映射到公共运行；按TRD §9.1暂停旧触发并收敛在途后切换owner_generation，停止独立Brain发现/worker调度循环。Brain status/jobs/control保留兼容API并转发公共服务。
3. 完成“手动或定时触发→公共run/attempt→受限模型→Work Unit/候选事务提交→事件/产物链接”的纵向链。回填以根run聚合有界子步骤，原范围与进度可暂停/恢复，自动重试不制造侧栏新任务。
4. 验证切换前后崩溃、重复启动和回滚；旧worker即使收到晚到模型结果也无法写回。无法收敛的进程或删除清理必须保持迁移未完成，不以重置进度绕过。

验证检查点：

- cargo test -p screenpipe-engine --test task_migration brain -- --nocapture（新增隔离迁移套件）
- cargo test -p screenpipe-engine --test brain_correctness extraction -- --nocapture
- 桌面目录：bun run test:tauri brain_runtime -- --nocapture；覆盖restart/sleep、晚到模型返回及根run/attempt事件映射。
- 对比旧job ID/状态/输入/已用预算和新映射，确认未选择的历史不进入任务；同一逻辑输入只有一份有效产物，删除后消息重放无正文。

交接信号：Brain实际只由公共任务服务调度，重启后的模型计数、状态、进度及运行消息一致；可作为其他类型迁移的可复用纵向样例。

### F14 · 活动总结、办公同步与用户Pipes统一接管

```yaml
inherits: node_defaults
plan_unit_id: F14
depends_on:
- F03
- F07
- F13
resolves:
- SCOPE-R12
- SCOPE-R13
acceptance_ids:
- AC-R1-04
- AC-R9-04
- AC-R12-03
- AC-R13-01
- AC-R13-02
- AC-R13-03
- AC-R13-04
- AC-R13-05
write_ownership:
- crates/screenpipe-core/src/tasks/
- crates/screenpipe-db/src/db/tasks/
- crates/screenpipe-engine/src/tasks/
- crates/screenpipe-core/src/lib.rs
- crates/screenpipe-db/src/db/mod.rs
- crates/screenpipe-engine/src/lib.rs
- crates/screenpipe-db/src/migrations/*tasks*.sql
- crates/screenpipe-core/src/pipes/mod.rs
- crates/screenpipe-core/assets/pipes/meeting-summary/pipe.md
- crates/screenpipe-core/src/pipes/builtin_migrations.rs
- crates/screenpipe-engine/src/pipe_store.rs
- crates/screenpipe-engine/src/pipes_api.rs
- crates/screenpipe-engine/src/routes/pipe_store.rs
- crates/screenpipe-engine/src/brain/office.rs
- crates/screenpipe-engine/src/brain/office_routes.rs
- crates/screenpipe-db/src/db/brain/office.rs
- apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs
- apps/screenpipe-app-tauri/src-tauri/src/office_runtime.rs
- apps/screenpipe-app-tauri/src-tauri/src/brain_runtime.rs
- apps/screenpipe-app-tauri/src-tauri/src/main.rs
- crates/screenpipe-engine/tests/task_migration.rs
- crates/screenpipe-engine/tests/task_contracts.rs
- crates/screenpipe-engine/src/server.rs
mutex:
- task-contracts
- pipes-runtime
- history-writer
- office-contracts
- db-writer
- desktop-lifecycle
parallel_mode: serial_shared_writer
required_skills:
- agent-brain
- codebase-analysis
- refactor-plan
```

1. 活动总结沿用F03迁库后的内容/coverage服务，配置接入公共内置定义，迁移原enabled/interval/next-run；办公同步用连接配置引用注册任务，接管15分钟等旧业务定时器，保留provider/account/range revision与分页cursor。
2. 适配PipeManager/PipeStore到公共运行服务：保留pipe.md、旧API/执行ID、事件去重、手动/定时触发、原权限/输出/历史以及显式既有策略。用户任务与内置任务用不同handler，不继承Brain审核规则，也不让Brain继承任意工具权限。
3. 对每一类型逐个停止旧触发、收敛在途、校验历史/配置、原子切换所有者，最后移除重复调度/内存运行真源；触发按原去重/补跑语义持久接收，不丢事件或改变外部副作用次数。任务未知写结果进入待确认，禁止自动重跑可能已成功的外部动作。
4. 将运行日志/消息转换为公共事件及legacy映射，保留旧历史入口和来源删除能力。运行状态来自公共持久记录；office纯I/O无需启动模型，活动/Brain/用户Pipes模型调用受F09共享限制。
5. 使用支持新表/删除journal的兼容适配器验证逐类型回滚，不重置完成记录或恢复旧正文。采集循环、UI轮询及live-view既有行为保持当前边界，不扩大全仓定时器重构。

验证检查点：

- cargo test -p screenpipe-engine --test task_migration -- --nocapture
- cargo test -p screenpipe-engine --test task_contracts compatibility -- --nocapture
- cargo test -p screenpipe-core --lib pipes；桌面目录分别bun run test:tauri activity_history -- --nocapture、bun run test:tauri office_runtime -- --nocapture。
- 四类机制的配置/历史ID集合/状态/游标逐项对照；手动/定时/事件/重启/取消/过期租约/迁移中触发正反用例；用fake外部写入回执计数证明结果未知不自动重放。
- 满载聊天+Brain+用户Pipes+两平台同步的隔离场景验证资源限额、嵌套调用无死锁、无跨账号取消；旧API和历史链接均可访问，无双调度/状态双写路径。

交接信号：所有纳入类型共享任务、运行和资源基础能力，各业务规则与旧Pipes能力保留；公共服务实际拥有唯一触发权，现行UI可基于同一状态服务接入。

## 6. 验收映射和证据口径

当前PRD是45项验收阈值真源；原报告保留37项历史问题证据，R12/R13记录本次新增8项。本文是执行次序真源，F01建立全量契约基线，以下表格列具体修复/验收责任；F11汇总不替代局部测试。P01–P11仍是回归案例，不另算产品验收；不得把新增基础改造追记为旧报告已验证。

| 验收 | 修复/验收节点 |
|---|---|
| AC-R1-01 | F04, F08, F11 |
| AC-R1-02 | F02, F04, F13, F11 |
| AC-R1-03 | F02, F04, F11 |
| AC-R1-04 | F03, F04, F14, F11 |
| AC-R2-01 | F04, F08, F11 |
| AC-R2-02 | F05, F10, F11 |
| AC-R2-03 | F02, F05, F10, F11 |
| AC-R3-01 | F06, F11 |
| AC-R3-02 | F02, F06, F11 |
| AC-R3-03 | F06, F11 |
| AC-R4-01 | F02, F06, F11 |
| AC-R5-01 | F06, F10, F11 |
| AC-R5-02 | F06, F11 |
| AC-R5-03 | F02, F05, F06, F11 |
| AC-R6-01 | F02, F07, F11 |
| AC-R6-02 | F02, F03, F04, F11 |
| AC-R6-03 | F02, F11 |
| AC-R6-04 | F09, F07, F11 |
| AC-R7-01 | F05, F06, F10, F11 |
| AC-R7-02 | F02, F05, F10, F11 |
| AC-R8-01 | F09, F04, F13, F11 |
| AC-R8-02 | F09, F04, F13, F11 |
| AC-R8-03 | F03, F04, F13, F10, F11 |
| AC-R9-01 | F07, F08, F11 |
| AC-R9-02 | F02, F07, F08, F11 |
| AC-R9-03 | F02, F04, F08, F11 |
| AC-R9-04 | F07, F14, F10, F11 |
| AC-R11-01 | F10, F11 |
| AC-R11-02 | F10, F11 |
| AC-R11-03 | F10, F11 |
| AC-R12-01 | F05, F08, F10, F11 |
| AC-R12-02 | F06, F10, F11 |
| AC-R12-03 | F12, F13, F14, F10, F11 |
| AC-R13-01 | F12, F07, F14, F10, F11 |
| AC-R13-02 | F12, F13, F07, F14, F10, F11 |
| AC-R13-03 | F09, F14, F11 |
| AC-R13-04 | F13, F14, F11 |
| AC-R13-05 | F02, F12, F09, F13, F14, F10, F11 |
| AC-EVAL-01 | F11 |
| AC-EVAL-02 | F04, F08, F11 |
| AC-EVAL-03 | F06, F11 |
| AC-EVAL-04 | F11 |
| AC-EVAL-05 | F10, F11 |
| AC-EVAL-06 | F09, F11 |
| AC-EVAL-07 | F08, F11 |

原生测试必须走仓库 native queue/cache；不可用时记录该检查未执行，继续不依赖它的工作，不走 raw cargo 回退。开发阶段使用隔离夹具；真实数据/设备验收按F11及已有用户授权进行，不直接改写用户数据库。真实权限/账号不支持仅影响相应 F11 场景，不妨碍 F01–F10及F12–F14的合成测试和实现修复。

每条机器证据至少记录 command/cwd/exit_code/test_count/git_head/plan_hash/fixture_hash；UI 附状态前后与预期动作，模型评测附实际 Runtime/Preset/model 身份，不附秘密。未来使用的源文件必须当前核验，不能沿用旧执行计划已经漂移的 hash。

## 7. Remote Handoff Inputs

本轮按用户最新指示在当前会话串行实施，不安排子智能体，也不创建 worktree 委派。主执行者直接负责代码修改、审查、验证及计划调整；共享 DB/路由/模型资源仍保持单 writer。后续若用户明确委派，F06 的 MCP 测试、F08 的解析夹具可在共享 DTO 冻结后拆成专属 worktree；输入仅为该节点、TRD 对应章节、脱敏/合成夹具、allowed_paths、hash 和验证命令。

不得携带真实录制、会议正文、客户材料、CLI 登录文件、API key 或保留集答案。授权/Keychain、原生生命周期、桌面/浏览器和真实质量验收由本机主执行者承担。

## 8. 开工交接与结束条件

收到后续实施指令后，先核验本文件当前完整 SHA-256 和基线漂移，运行 `delivery-readiness` 的 plan_to_build 检查，由 agent-brain 创建带 plan_id/plan_unit_id/source hash 的 Task Pack，再按节点 Build/Verify/Acceptance。任务与路径由 Task Pack 收窄；本次已进入Task Pack交接，旧实现审查blocked仍作为缺陷证据，不能冒充产品通过。

计划修改后重算完整文件 hash，更新下游引用，不把本文件的 hash 存入自身 frontmatter。F11 完成前不把“已有代码/测试通过若干”写为“首版完成”；若仍有失败或未测，记录责任节点与下一步，保留本次历史评审及反例。

## 实施进度记录

- 2026-09-08：F01基线通过主Agent审查及5项任务检查，DB 5例通过，Engine 1个正常对照通过/10个已知缺陷仍失败；不代表产品45项验收通过。当前进入F02。
- F02范围补正：实码确认恢复在Worker启动后后台执行，增加共享状态、server、Tauri启动、routes与answer入口的写归属；删除恢复必须先于服务可读，保持原14节点DAG与产品范围。

- 2026-09-08执行方式调整：按用户最新指示，以B01包装现有14节点，由当前会话直接连续实现并完成必要自测，不安排子智能体，最后统一审查；执行中只记录真实范围/环境阻塞。F01已验收，F02候选及第一轮审查记录保留，后续不再逐节点交回。F12补充screenpipe-db依赖清单与必要Cargo.lock写归属，公共类型不得通过重复定义规避依赖声明。
- 2026-09-08实现记录：公共任务 worker 已接管桌面 Brain 的抽取/编译/回填/办公同步领取；旧 `brain_jobs` 保留为兼容执行记录并与 public run 共享 lease/token/预算/终态。F14 的活动总结和用户 Pipe 已接入 public run 生命周期；真实 OAuth、模型、办公采样、桌面手测与完整45项产品验收仍未执行，计划保持 `in_progress`。
- 2026-09-08一致性补正：统一任务 API 的暂停/恢复/取消/重试现在与 `brain_jobs` 在同一 SQLite 事务内同步；运行中的暂停由 public worker 在安全边界释放共享 lease，OfficeSync 也可通过公共任务入口创建兼容 run。隔离迁移回归扩展为5例通过。
- 2026-09-08 F11审计：补齐 Office fake-CLI 生命周期/内容集成测试、前端 Brain API/任务事件契约测试及知识库入口 E2E smoke；Rust/TypeScript/MCP 隔离验证通过。45项矩阵已逐项建立，但真实模型/办公账号/桌面流程/冻结质量集/性能与审核观察尚未执行，全部保留 `unverified`，计划继续 `implementation: in_progress`、`product_acceptance: not_run`。
- 2026-09-08 F11桌面复核：`build:tauri:e2e` 原生构建与 `local-brain-entry-split.spec.ts` 两项 WebDriver smoke 均通过；同时修正 E2E helper 对 `pi_start_inner` 旧参数签名的调用。该结果仅覆盖知识库/自动化入口拆分，不改变45项真实产品验收仍为 `unverified` 的边界。
- 2026-09-09 复验与真机数据闭环（用户授权驱动 dev 库）：当轮独立复跑全部合成检查点全绿（Rust 10 套件、MCP 92、桌面 Vitest 117、test:tauri 四套件）；`local-brain-entry-split` 真机 2/2，legacy e2e 22 失败归因为基线前漂移（英文断言/旧 DOM，均在写归属外）。重建应用后以真实 dev 库+真实模型（Pi→MiniMax-M3）完成 AC-R7-01 全链：提问只引 v3→有误反馈自动暂停→CAS 建 v4→自审发布→再问只引 v4、无旧版召回；UI 可达性状态与库一致。发现并修复缺陷：发布纠正版不清反馈暂停导致闭环无法收口（`brain_publish_version` 同事务置 `paused=0`；回归测试 `publication_db_publish_lifts_feedback_pause_so_the_correction_loop_recloses`，TDD 红绿验证），修复后 db brain_correctness 8/8、engine 13/13、libs 全绿。**AC-R7-01 升为 passed**（45 项：passed=1 / unverified=44）；冻结质量集、办公真实样本、5 工作日观察仍 unverified。
