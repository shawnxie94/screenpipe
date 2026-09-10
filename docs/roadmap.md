# screenpipe / zh-local Roadmap

<!-- doc-covers: crates/screenpipe-core/src/tasks, crates/screenpipe-db/src/db/brain, crates/screenpipe-db/src/migrations, crates/screenpipe-engine/src/brain, crates/screenpipe-engine/src/tasks, crates/screenpipe-connect/src, apps/screenpipe-app-tauri/components/brain -->
<!-- doc-verified: 18b2379be -->

<!-- doc-covers 必须是 git pathspec，不能写逗号分隔的散文；散文永远匹配 0 个提交，会让漂移门禁空转。本文件只把领域包与统一任务面当作自己的覆盖范围，不覆盖上游采集/桌面全仓，避免门禁长期漂红。 -->

最近更新：2026-09-10。

本文件是 `screenpipe` `zh-local` 的统一产品与迭代入口。后续功能直接在该分支继续演进，不再维护独立的 MIT 拆分版 `zhiji`。历史拆分、迁移和验证材料保存在 [zhiji archive](archive/zhiji/README.md)，只作为决策记录，不作为当前实现基线。

## 当前产品方向

`screenpipe` 是本地优先的个人工作记录、知识沉淀与研究工作台。优先满足日常使用，不以当前不明确的商业化前景为理由重做一套干净 MIT 产品。

核心闭环：

```text
屏幕 / 音频 / 浏览器 / RSS / 文件
                ↓
        本地原始记录与信息对象
                ↓
        检索、证据、知识与研究产物
                ↓
        目标、行动、完成证据与复盘
                ↓
        可选的公众发布与订阅
```

### 统一整合拓扑

```text
Infinitum radar / RSS ──┐
                        ├─→ screenpipe zh-local ──→ Lumina publication
Lumina browser capture ─┘             ↑
                                       │
                           Personal OS file adapter
```

- `screenpipe zh-local` 是唯一持续开发的产品和事实中枢，并提供跨领域的 Memory、Evidence、Retrieval 和 Lifecycle Core 能力。
- `Local Brain` 是首个记忆领域包：保留活动/情境记忆，同时把高价值内容编译为 Work Unit、知识候选、版本、引用问答和审核后的长期记忆；其他领域包也可使用同一套 Core Memory。
- `zhihuan` 提供平台、Workflow、Artifact、审批、审计和持续研究的设计输入；不恢复独立产品。
- `Personal OS` 通过适配器接入，Goal/Quest/Review 继续以文件协议为真源。
- `Infinitum` 负责 RSS、清洗、去重、质量判断和简单聚合；复杂综合在 screenpipe Workflow 中完成。
- `Lumina` 的新网页采集直接进入 screenpipe；已审核内容按需单向发布到 Lumina，不做双向业务同步。

Core 只保留通用对象信封、Memory、来源/证据、版本、关系、Workflow/Run、审核、生命周期和发布边界；领域对象和领域记忆通过内置 Domain Pack 注册，不把 Article、日报、Goal 或 Quest 硬编码为来源产品特判。先实现内置扩展，不提前建设第三方插件市场。

## 已完成基线

- README 与项目文档已切换为本地优先的个人知识库定位。
- Local Brain 基础改造、知识库五入口、统一任务事件和纠错闭环已在 `zh-local` 持续落地。
- 现有 Local Brain PRD/TRD、正确性计划、验收矩阵和真实环境证据继续作为当前开发输入。
- 原独立 `zhiji` 项目的整合框架、迁移材料和历史验证记录已转入本仓库的归档目录；不再以独立应用身份、MIT 基线或独立数据库作为后续目标。

## 进行中

- **Local Brain 首版基础改造与修复**：继续执行 `docs/plans/personal-brain-correctness-execution-plan.md`。删除/恢复、公共任务、Brain 纵向接管、活动/办公/Pipes 迁移已随 B01 落地（`5f605284c`、`4392b9fe9`，2026-09-09，合成检查点全绿）；剩下的是真实样本验收（45 项矩阵 passed=1 / unverified=44）与 B01 收口审查，不再把这四项当待办。
- **进度口径**：上述计划的 `implementation.completed_units` 只记 `[F01]`，是逐节点审查未交回的滞后字段；进度以该计划文末「实施进度记录」为准，待 B01 收口统一回写。
- **桌面端和中文工作台稳定性**：继续修复采集、音频、聊天、Pipe、连接、状态恢复和前端中文路径；沿用当前 `zh-local` 的上游优化，不回移到历史 MIT 快照。
- **整合边界收敛**：新需求先判断属于 Core、Domain Pack、Connector、Workflow 或 Publication Adapter；禁止为 Infinitum、Lumina 或 zhihuan 再建立第二套事实源、任务调度或审批状态。

## 后续阶段

阶段顺序、优先级和进度在本文件维护；各阶段**契约冻结顺序与依赖**、Core 公共语义和未决项（O-01–O-06）见 [个人工作台整合框架](trd/personal-workbench-integration-framework.md) §5/§6，本文件不重复其内容。

### P1：完成当前 Local Brain 闭环

- 保持 screenpipe 采集、SQLite/FTS、活动记录和现有应用身份。
- 完成 Work Unit → Knowledge Candidate → Review → Published Knowledge。
- 完成引用问答、版本修订、删除传播、任务恢复和真实样本验收。
- 不在本阶段引入 Infinitum/Lumina 全量页面，也不重做基础采集架构。

### P2：建立通用扩展与 Lumina 直采

- 先冻结跨阶段契约（对象信封与 scope 语义、Relation 最小范围、任务种类注册清单）：冻结顺序、触发时点与判定用例见[整合框架](trd/personal-workbench-integration-framework.md) §5/§6，本文件不复述。
- 将 Lumina 浏览器扩展改为 screenpipe 的采集客户端；新网页直接进入 screenpipe。外部扩展的最低版本、不可用时的降级路径（手动粘贴或文件导入）在 P2 计划中声明。
- 将文章抽取、阅读、批注、摘要、翻译等能力接入 `reading-pack`，优先服务知识编译。
- Lumina 旧数据只做受控导入或只读核对，不作为新内容事实源。

### P3：接入 Personal OS 行动闭环

- 通过 `personal-pack` + File Ledger Adapter 接入 Goal、Quest、Review。
- Personal OS 继续维护文件真源和现有域隔离；screenpipe 只保存关系、投影和运行引用。
- 区分支持事实的 Source Evidence 与证明行动完成的 Completion Record。
- 验证行动建议、用户确认、完成留证和复盘往返，不让模型自动修改正式行动状态。

### P4：研究综合、Infinitum 雷达与 Lumina 发布

- 以 `research-pack` 吸收 zhihuan 的持续研究、Artifact、审核和受控执行设计。
- 以 `radar-pack` 接入 Infinitum RSS、清洗、去重、基础聚合和候选信号。
- 在 screenpipe Workflow 中实现目标相关性、历史比较、证据化周期简报和行动建议；Infinitum 原日报仅作为回归参考，不再继续扩展为综合真源。
- 将已审核 Artifact 通过单向 Publication Adapter 发布到 Lumina，支持版本、撤回、可见性和失败重试。

### P5：真实数据切换与旧入口收敛

- 完成历史 Lumina/Infinitum 数据的受控导入、来源命名空间、版本和删除传播验证。
- 决定 Infinitum 是否继续作为独立雷达运行、Lumina 是否继续作为独立公众站点。
- 在真实日常使用稳定后，再收敛安装、备份、恢复、升级和旧入口。

P2 的契约冻结批（信封与 scope、Relation、注册清单）必须先于 P2 采集实现、P3、P4；在此前提下，P2 采集实现与 P3 可对调，P4/P5 内部顺序可按真实使用调整。调整时在本文件记录理由；涉及接口或数据真源变化时同步更新整合框架。

## 不纳入当前默认范围

- 重新拆分干净 MIT 版本或追赶上游独立分支。
- 商业化分发、第三方插件市场、团队协作、云同步和计费。
- Infinitum、Lumina、zhihuan 的全部页面、后台和数据库整体合并。
- 双向办公写入、自动发布高风险操作和多 Runtime 调度。
- Windows/Linux、商业分发和新增 Runtime；另行立项。
