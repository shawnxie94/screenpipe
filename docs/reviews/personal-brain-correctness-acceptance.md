# 知迹 · Local Brain 正确性验收矩阵

> 本文件是 F11 的当前批次快照，不把已有代码、隔离测试或旧报告自动升级为产品验收通过。

- 计划：`docs/plans/personal-brain-correctness-execution-plan.md`
- 计划 SHA-256：`1930a41d283ea4ae1d8a1ec613bd73519ade5830c8076b834fb9f327278c426e`
- 验证 HEAD：`0a83343ac7e75cc67effa3dae4430bcb4dd825cf`
- 执行方式：当前会话单 writer；未安排子智能体；未 commit/push。
- 当前汇总：`passed=1`，`failed=0`，`unverified=44`，`accepted_risk=0`。
- 解释：本轮已经补齐和验证了大量实现级契约，并通过了知识库/自动化入口拆分的桌面 smoke；但 PRD 要求的真实语料、真实账号/模型、完整桌面业务流程、性能/SLA 和人工质量阈值尚未在当前批次冻结并执行，因此 45 项整体验收均保留为 `unverified`。

本地证据索引见 [`local-verification-2026-09-08.md`](evidence/personal-brain-correctness/local-verification-2026-09-08.md)。状态含义严格按整条 AC 判断：局部测试通过不等于该 AC 全部通过；没有把“旧报告曾通过”继承为当前代码证据。

| 验收项 | 状态 | 当前证据 | 未完成的完整验收缺口 |
|---|---|---|---|
| AC-R1-01 | unverified | Engine/DB correctness tests | 冻结一天真实数据并达到 ≥80% 合法 Work Unit |
| AC-R1-02 | unverified | DB idempotency/revision tests | 真实迟到转写、崩溃重启和人工改分类闭环 |
| AC-R1-03 | unverified | deletion barrier tests | 四类真实来源及办公导入派生正文全链路检查 |
| AC-R1-04 | unverified | migration/backfill contract tests | 迁移后真实旧数据 coverage 与首次启用边界核对 |
| AC-R2-01 | unverified | evidence-derived SOP unit test | ≥3 独立真实会话与人工确认可用性 |
| AC-R2-02 | unverified | knowledge version/CAS tests | v1→v2、废止、到期复核真实 UI 演示 |
| AC-R2-03 | unverified | publish support/deletion guards | 编辑无支持事实、归档和发布竞态的完整 UI 验证 |
| AC-R3-01 | unverified | search failure/filter regression tests | 旧 `/search` 全部兼容维度与无外部依赖实机回归 |
| AC-R3-02 | unverified | source/app/version filter tests | 多应用跨日期真实语料、缓存失效和版本切换 |
| AC-R3-03 | unverified | partial/all-failure typed route tests | 固定查询集相关性与三种失败状态人工核验 |
| AC-R4-01 | unverified | local memory/API tests | MCP、文件出口和受管文件失败重试实机闭环 |
| AC-R5-01 | unverified | answer/source contract and MCP tests | 冻结语料逐 claim 引用核验 |
| AC-R5-02 | unverified | no-evidence/conflict/deletion tests | 提示注入、冲突、过期和回答率人工集 |
| AC-R5-03 | unverified | answer deletion/version guards | 生成期间切版/删除的 REST 与 MCP 双入口实测 |
| AC-R6-01 | unverified | deletion and emission guard tests | frame/audio/ui_event/memory/办公五类真实派生清理 |
| AC-R6-02 | unverified | deletion barrier and recovery tests | 发布、索引重建、重启、迁库回退并发删除 |
| AC-R6-03 | unverified | retention preservation test | 三类 retention 矩阵和共享豁免释放实测 |
| AC-R6-04 | unverified | typed outlet/error tests | 设置/连接清单与所有已启用出口无密钥实查 |
| AC-R7-01 | **passed**（2026-09-09 真机 dev 库） | 真实数据完整闭环：真实模型提问只引 v3→有误反馈自动暂停（反馈表 `located` 可追溯）→CAS 创建候选 v4→自审发布→再次提问只引 v4、无旧版召回；UI 可达性树与库状态一致；发现并修复"发布不清反馈暂停"缺陷（TDD 红绿+回归测试） | 遗留说明：操作经 UI 同源 REST 驱动，未逐键点击；无碍本项演示语义 |
| AC-R7-02 | unverified | feedback DTO/locator tests | 无法定位反馈、驳回/恢复与删除后的界面正文清理 |
| AC-R8-01 | unverified | task lease/retry/recovery tests | 关窗、退出、休眠、断网、非法输出、预算真实状态 |
| AC-R8-02 | unverified | cancellation and task migration tests | 长批期间交互等待、取消/删除的桌面实测 |
| AC-R8-03 | unverified | backfill range contract tests | 首次启用边界、迟到来源与 7 天范围真实数据核验 |
| AC-R9-01 | unverified | Office fake CLI/content fixtures | 两工具冻结版本/账号/样本，真实内容 ≥90% 导入与中文检索 |
| AC-R9-02 | unverified | Office lifecycle/content tests | 真实依赖缺失、权限、限流、离线和断开/取消 |
| AC-R9-03 | unverified | office dedupe/source tests | ≥10 会话、跨工具 Work Unit→SOP→问答→删除 |
| AC-R9-04 | unverified | connection DTO/frontend tests | 连接页授权、范围、同步、重启持久化的桌面流程 |
| AC-R11-01 | unverified | frontend label regression and E2E spec added；2026-09-09 真机 e2e entry-split 2/2 | 启动/引导/错误/空状态完整画面核对 |
| AC-R11-02 | unverified | retained identifier inventory；2026-09-09 真机验证知识库可访问名称与标签 | 全前端品牌残留扫描和无障碍/通知/标题逐项画面核对 |
| AC-R11-03 | unverified | frontend typecheck | CLI/API/MCP/深链/存储/系统权限在桌面发布包中回归 |
| AC-EVAL-01 | unverified | no real holdout run in this batch | ≥10 真实会话、≥3 重复流程、30–50 冻结查询 |
| AC-EVAL-02 | unverified | local extraction/compile tests | 真实语料 ≥90% 事实支持、可用 SOP 和流程变体 |
| AC-EVAL-03 | unverified | answer citation/error tests | 保留集无依据结论 0、可回答率 ≥80% |
| AC-EVAL-04 | unverified | local lifecycle regressions | 真实 SOP v1→纠错→v2→删除及全部入口兼容 |
| AC-EVAL-05 | unverified | no burden observation | 固定实际 Preset/Runtime、5 工作日审核负担和积压 |
| AC-EVAL-06 | unverified | MCP/native tool tests | 所有已启用 Pi/文件/MCP 出口可观测验证 |
| AC-EVAL-07 | unverified | Office fixtures and connection code | 两款工具真实账号/场景、腾讯真实转写时间回链 |
| AC-R12-01 | unverified | KnowledgeHub/API/frontend tests | 工作单元→活动/证据→知识→支持会话的桌面闭环 |
| AC-R12-02 | unverified | answer/MCP/frontend contract tests；2026-09-09 真机完成 REST 侧 v3有误→v4修订→重问闭环（见 AC-R7-01） | 从知识库入口发起提问、生成期间切版/删除、历史消息失效与过滤传递的桌面实测 |
| AC-R12-03 | unverified | task event and activity code tests | 退出重启、run 聚合、非模型事件和删除后消息实测 |
| AC-R13-01 | unverified | unified definitions/API tests | 自动化与连接单一配置真源的桌面联动 |
| AC-R13-02 | unverified | run CAS/lease/control/migration tests | 手动/定时/事件全触发及重启/失租约/超时实测 |
| AC-R13-03 | unverified | task resource fields and lease tests | 聊天/Brain/Pipe 资源竞争、办公独立限额和预算跨重启 |
| AC-R13-04 | unverified | public owner migration tests | 四类旧机制切换、崩溃/回滚/旧 API 兼容实测 |
| AC-R13-05 | unverified | deletion-safe task/run tests | 产物、进度、完成事件同事务及无法清理待重试出口 |

## 当前阻塞与下一步

正式 F11 不能通过的阻塞不是本地编译错误，而是尚未执行的真实验收输入：模型/Runtime 实际绑定、飞书和腾讯会议真实样本、桌面 native/WebDriver、冻结保留集以及性能/审核观察。下一步应在具备这些外部条件后，逐项重跑并只把有完整证据的行改为 `passed`；不以“代码已实现”替代上述证据。

## 2026-09-08 复验记录

同日稍晚在未变更的工作树上独立复跑全部 F11 合成检查点：Rust 10 个套件、MCP 测试/typecheck、桌面 typecheck、聚焦 Vitest（117 例）、4 个 test:tauri 原生套件全部通过；diff scope 复核 clean（83/119/2）；Task Pack 正式验收 `run-acceptance --manual-ok` 10/10 通过（implementation 级整批 lead review 通过）。全量前端套件中的 24 个失败经逐文件核实均为审查基线之前已存在的汉化断言漂移，全部位于 B01 写归属之外的文件，非本批次引入；详见 [证据文件](evidence/personal-brain-correctness/local-verification-2026-09-08.md)。上表 45 项维持 `unverified` 不变：真实模型/办公账号/冻结质量集/性能与审核观察仍未执行。

**2026-09-09 凌晨桌面真机验证**：重新 `build:tauri:e2e` 构建成功并以 WebDriver 驱动真实桌面 WebView——计划冻结的 `local-brain-entry-split` 在真机上 **2/2 通过**（知识库入口名、工作单元/知识/画布三标签、无提问/运行状态残留、自动化独立入口）。另尝试的 4 个 legacy e2e（brain-overview/settings-sections/pi-extensions/chat-rich-result-cards）失败经归因均为基线前漂移：英文标签断言落后于汉化提交、`section-brain` 旧 DOM 落后于计划要求的入口拆分，全部在本批写归属之外。AC-R11-01/02 的证据列已由真机运行加强，但"错误/空状态完整画面核对"仍未执行，行状态保持 `unverified`。

**2026-09-09 真机 dev 库纠错闭环（用户授权）**：重建应用后以 `SCREENPIPE_DATA_DIR=~/.screenpipe-dev` 启动真实应用，走真实模型（Pi→MiniMax-M3）在真实知识 `walkthrough-sop-1` 上完成 AC-R7-01 全链：提问只引 v3→有误反馈自动暂停→CAS 创建候选 v4→自审发布→再问只引 v4、无旧版召回；UI 可达性树与库状态一致；反馈表 `located` 可追溯。过程中发现并修复真实缺陷：发布纠正版不清除反馈暂停导致闭环无法收口（`brain_publish_version` 同事务置 `paused=0`，TDD 红绿验证+回归测试，修复后全套件绿、真机复跑闭环通过）。**AC-R7-01 升级为 `passed`**，其余 44 项保持 `unverified`。详见证据文件"Real-data correction loop"一节。

## 2026-09-11 活动层收口与内部 skill（B01–B04b）实现记录

P1 收尾遗留项的引擎侧全部完成；本节为代码级收口记录，不改变任何 `unverified` 行状态（真实环境判据见 runbook）：

- **B01** 证据按变化保留 + 丢弃记账 + 摘要表与查询面（`9595528ea`）
- **B02a/b** 单一总结器、摘要生产者、时间线 DB 摘要优先、停用旧叙事自动生成（`96fde4b0a`、`ad5c54155`、`5f5aeae6e`）
- **B03a/b** WorkUnit 先摘要后召回（schema v2）、会话计数按活动去重（`ff920587c`、`9c7b8bff2`）
- **B04a** 四个内部 skill（共享取数 + 活动总结/工作单元/知识提炼）：中性真源、无品牌、接口真实（nest+method 校验与 8 条负例回归）、Pi/ACP/AgentLayout 注入（`8dee0a5dd`、`9e64edd5c`）
- **B04b** skill 哈希进输入指纹（`compute_input_hash` 增 `skill_revision` 成分，extract/compile/summarize 三处透传）；查询轨迹与强制上限（`knowledge_query_trace` 表、10 个读接口 opt-in `X-Screenpipe-Trace`、200/键 429、同窗口重跑结果指纹一致）（`beda105041b8`）

对 R1/R2/R13 的影响：这些行描述的输入层已随活动层实现替换；按 roadmap 约定不为旧设计重复取证，真实环境判据（冻结语料、真实 UI、桌面联动）保持 `unverified`，执行入口：[p1-closure-runbook](evidence/p1-closure-runbook-2026-09-11.md)。证据目录：`evidence/activity-retention-2026-09-11/`（B01）、`evidence/activity-summary-b02a-2026-09-11/`、`evidence/activity-read-source-b02b1-2026-09-11/`、`evidence/activity-retire-legacy-b02b2a-2026-09-11/`、`evidence/workunit-recall-b03a-2026-09-11/`、`evidence/session-count-b03b-2026-09-11/`、`evidence/internal-skills-b04a-2026-09-11/`、`evidence/internal-skills-b04b-2026-09-11/`。
