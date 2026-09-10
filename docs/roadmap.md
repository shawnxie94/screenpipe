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

外部产品（Lumina、Infinitum）的能力迁移后置：先建 zh-local 核心能力（信封、领域包注册、scope 与放行、Relation），接入时点见后续阶段；延后期间两者继续独立运行，其数据不作为 screenpipe 的事实源，只有显式导入才成为来源（框架 D-12）。

Core 只保留通用对象信封、Memory、来源/证据、版本、关系、Workflow/Run、审核、生命周期和发布边界；领域对象和领域记忆通过内置 Domain Pack 注册，不把 Article、日报、Goal 或 Quest 硬编码为来源产品特判。先实现内置扩展，不提前建设第三方插件市场。

## 已完成基线

- README 与项目文档已切换为本地优先的个人知识库定位。
- Local Brain 基础改造、知识库五入口、统一任务事件和纠错闭环已在 `zh-local` 持续落地。
- 现有 Local Brain PRD/TRD、正确性计划、验收矩阵和真实环境证据继续作为当前开发输入。
- 原独立 `zhiji` 项目的整合框架、迁移材料和历史验证记录已转入本仓库的归档目录；不再以独立应用身份、MIT 基线或独立数据库作为后续目标。

## 进行中

- **Local Brain 首版基础改造与修复**：继续执行 `docs/plans/personal-brain-correctness-execution-plan.md`。删除/恢复、公共任务、Brain 纵向接管、活动/办公/Pipes 迁移已随 B01 落地（`5f605284c`、`4392b9fe9`，2026-09-09，合成检查点全绿）；剩下的是真实样本验收（45 项矩阵 passed=1 / unverified=44）与 B01 收口审查，不再把这四项当待办。
- **进度口径**：上述计划的 `implementation.completed_units` 只记 `[F01]`，是逐节点审查未交回的滞后字段；进度以该计划文末「实施进度记录」为准，待 B01 收口统一回写。
- **P1 收尾的遗留项**（与验收一并完成，算 P1 范围，不新开阶段；决策见框架 D-13–D-15）：
  - 活动层合并：取消“活动台账 + 活动历史”两套总结逻辑，只留一套（间隔 → 摘要 + 关键字 + 原始引用）
  - 证据保留改为按变化采样，不再只留每段首尾两条；被丢弃的观测记录条数与原因
  - 摘要自足口径（长度分档 + 必含要素 + 5–12 个关键字）；会话计数按活动去重；旧叙事标 legacy、不参与新抽取
  - 活动生成的取数改为执行体自己查（只读工具 + 内部 skill），任务只给指令；修正现行提示词“系统说用 API、用户提示禁止调工具”的矛盾
  - WorkUnit 改为“先读摘要、按需按关键字召回原文”，字段新增流程/环境/细节（schema v2）
  - 读源从本地加密存储收敛到数据库（消灭双份状态）
  - 受影响验收项（R1/R2/R13 里描述旧输入层的部分）在改造后重验，不为旧设计重复取证
- **桌面端和中文工作台稳定性**：继续修复采集、音频、聊天、Pipe、连接、状态恢复和前端中文路径；沿用当前 `zh-local` 的上游优化，不回移到历史 MIT 快照。
- **整合边界收敛**：新需求先判断属于 Core、Domain Pack、Connector、Workflow 或 Publication Adapter；禁止为 Infinitum、Lumina 或 zhihuan 再建立第二套事实源、任务调度或审批状态。

## 后续阶段

阶段顺序、优先级和进度在本文件维护；各阶段**契约冻结顺序与依赖**、Core 公共语义和未决项（O-01–O-06）见 [个人工作台整合框架](trd/personal-workbench-integration-framework.md) §5/§6，本文件不重复其内容。

开工门禁：新阶段计划只在上一阶段收口后建立（验收有真实证据，或用户具名接受风险）。P1 收口前不建 P2 计划、不开 P2 实现。

### P1：完成当前 Local Brain 闭环

- 保持 screenpipe 采集、SQLite/FTS、活动记录和现有应用身份。
- 完成 Work Unit → Knowledge Candidate → Review → Published Knowledge。
- 完成引用问答、版本修订、删除传播、任务恢复和真实样本验收。
- 不在本阶段引入 Infinitum/Lumina 全量页面，也不重做基础采集架构。

### P2：建立 zh-local 核心能力

- 先冻结跨阶段契约（对象信封与 scope 语义、Relation 最小范围、任务种类注册清单）：冻结顺序、触发时点与判定用例见[整合框架](trd/personal-workbench-integration-framework.md) §5/§6，本文件不复述。
- 落地 scope 迁移：来源表 `dataset_id` 归一为 scope 词表、`memories` 补 scope 列、检索引擎入口统一放行（默认只放 `personal`）；办公导入支持对象级改判（O-06）。
- 第一验证者用现有办公来源路径（飞书/腾讯会议），不引入新仓库或新采集客户端。
- 不迁移 Lumina/Infinitum 的采集与发布能力（见框架 D-12）。

### P3：接入 Personal OS 行动闭环

- 通过 `personal-pack` + File Ledger Adapter 接入 Goal、Quest、Review。
- Personal OS 继续维护文件真源和现有域隔离；screenpipe 只保存关系、投影和运行引用。
- 区分支持事实的 Source Evidence 与证明行动完成的 Completion Record。
- 验证行动建议、用户确认、完成留证和复盘往返，不让模型自动修改正式行动状态。

### P4：领域包接入与研究综合（reading / radar / research）

- 以 `research-pack` 吸收 zhihuan 的持续研究、Artifact、审核和受控执行设计。
- 以 `radar-pack` 接入 Infinitum RSS、清洗、去重、基础聚合和候选信号。
- 以 `reading-pack` 接入网页资料采集与阅读：将 Lumina 浏览器扩展改为 screenpipe 的采集客户端，新网页直接进入 screenpipe，旧数据受控导入；外部扩展的最低版本与不可用时的降级路径（手动粘贴或文件导入）在 P4 计划中声明。
- 在 screenpipe Workflow 中实现目标相关性、历史比较、证据化周期简报和行动建议；Infinitum 原日报仅作为回归参考，不再继续扩展为综合真源。
- 将已审核 Artifact 通过单向 Publication Adapter 发布到 Lumina，支持版本、撤回、可见性和失败重试。

### P5：真实数据切换与旧入口收敛

- 完成历史 Lumina/Infinitum 数据的受控导入、来源命名空间、版本和删除传播验证。
- 决定 Infinitum 是否继续作为独立雷达运行、Lumina 是否继续作为独立公众站点。
- 在真实日常使用稳定后，再收敛安装、备份、恢复、升级和旧入口。

P2 的契约冻结（信封与 scope、Relation、注册清单）必须先于 P3、P4；在此前提下 P3 与 P4 的顺序可按真实使用调整。Lumina/Infinitum 的能力迁移均在 P4 及之后（框架 D-12）；调整时在本文件记录理由，涉及接口或数据真源变化时同步更新整合框架。

## 不纳入当前默认范围

- 重新拆分干净 MIT 版本或追赶上游独立分支。
- 商业化分发、第三方插件市场、团队协作、云同步和计费。
- Infinitum、Lumina、zhihuan 的全部页面、后台和数据库整体合并。
- 双向办公写入、自动发布高风险操作和多 Runtime 调度。
- Windows/Linux、商业分发和新增 Runtime；另行立项。
