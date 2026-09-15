# screenpipe Roadmap

<!-- doc-covers: crates/screenpipe-capture, crates/screenpipe-screen, crates/screenpipe-audio, crates/screenpipe-a11y, crates/screenpipe-db/src/db/activity_ledger.rs, crates/screenpipe-db/src/migrations, crates/screenpipe-engine/src/connections_api.rs, crates/screenpipe-connect/src, crates/screenpipe-core/src/tasks, crates/screenpipe-engine/src/tasks -->
<!-- doc-verified: 000000000 -->

<!-- doc-covers 必须是 git pathspec，不能写逗号分隔的散文。本文件只覆盖采集、统一存储与接入（Connector）面，不覆盖桌面 UI 全仓，避免漂移门禁长期漂红。 -->

最近更新：2026-09-15。

本文件是 `screenpipe` 的统一产品与迭代入口。历史方向（Local Brain、知识沉淀、zhiji 拆分、个人工作台整合）已被移除，决策记录归档在 `docs/archive/local-brain/` 与 `docs/archive/zhiji/`，只作决策参考，不再构成当前实现基线。

## 当前产品方向

**screenpipe 是本地信息收集口**：持续采集本机产生的各类信息（屏幕、音频、可访问性），并把其他应用的已有数据（办公文档、日历、消息等）作为独立接入渠道拉入同一个本地存储与检索底座，供外部应用与 AI Agent 消费。它不内建记忆、知识或研究产品——那些交给下游应用通过 REST API / MCP / Pipes 取数自行实现。

核心闭环：

```text
屏幕 / 音频 / 可访问性 ──┐
                        ├──→ 本地统一存储与检索（SQLite / FTS / 文件）
飞书 / 腾讯会议 / 其他接入 ─┘          ↓
                             外部应用 / Agent（REST API / MCP / Pipes）
```

### 边界

- **属于 screenpipe**：本地采集、接入渠道、统一存储、全文检索、原始证据保留、供外部消费的 API/MCP。
- **不属于 screenpipe**：记忆、知识库、研究综合、行动闭环、发布/订阅。一律外部化，screenpipe 只提供事实和证据存取。
- 数据默认保存在本机（`~/.screenpipe`，开发时 `~/.screenpipe-dev`）；模型调用等外部数据流由用户显式配置。

## 已完成基线

- 屏幕 / 音频 / 可访问性 / OCR 采集链路持续运行。
- 本地 SQLite 持久化与全文检索（FTS），按时间、应用、窗口、内容类型查询。
- 活动 Ledger（确定性活动记录）与 Activity History。
- 本地 REST API（默认 `127.0.0.1:3030`）：搜索、帧、音频、健康、活动等。
- MCP 服务与 Pipes（Markdown 描述的个人 AI 工作流，结果存本机）。
- Tauri 桌面应用 + Rust CLI / 独立引擎。
- 飞书 / 腾讯会议接入（`screenpipe-connect` 的 office runner）已有可用的连接管理、增量同步与授权；以 `knowledge_jobs` 驱动的旧调度是待解耦项（见进行中）。
- 历史 Local Brain / 知识域已经整体退出当前范围，相关实现与验收进入归档，不再维护。

## 进行中

- **移除知识域（Local Brain）**：删除 engine `knowledge/`、db `knowledge/`、Tauri / 前端 knowledge、MCP 知识工具、CLI 知识 skills、活动层 AI 叙事，以及 `memories` 相关实现；保留 SQLite 迁移文件以保证 `user_version` 连续性。生产数据目录清空重建（本次方向调整允许全量重建，不保留知识数据）。
- **office 接入解耦为独立 Connector**：把飞书 / 腾讯会议从 knowledge 任务调度（worker / jobs / office_connections 表）中解放出来，改为独立同步循环与自有连接表，作为"其他应用数据接入"的样板渠道。
- **采集与桌面稳定性**：继续沿用当前 `zh-local` 分支的上游优化，不回移历史快照。

## 后续阶段

阶段顺序、优先级和进度在本文件维护；每个新阶段只在上一阶段收口（有真实证据或用户具名接受风险）后建立计划。

### S1：知识域清理收口

- 移除 knowledge / memories 代码、路由、前端入口与外部工具引用。
- office 接入提为独立 Connector（`/connections` 直接驱动同步，不经过 knowledge worker）。
- 全量重建本地数据目录；验证采集、检索、桌面应用在空库上正常启动。
- 更新 README、文档地图与 doc-covers，CI 门禁恢复绿色。

### S2：接入渠道扩展

- 以 office Connector 为样板，确立统一接入骨架：连接管理（授权 / 刷新 / 断开）、增量游标、来源登记、拉取落库。
- 按真实使用需要接入新渠道：浏览器采集、RSS/网页、日历、消息等。
- 不引入第二套事实源或任务调度；所有渠道统一写入同一本地存储与检索底座。

### S3：存储与检索深化

- 在采集 + 接入的数据规模上优化检索（FTS 分面、跨渠道按来源过滤、时间线聚合）。
- 明确原始证据生命周期（保留、压缩、删除传播）策略。
- 供外部应用稳定的 API/MCP 契约，按需演进 SDK。

### S4：桌面端与发布边界

- 桌面端回归"数据底座"定位：采集状态、渠道管理、检索浏览，移除知识相关界面遗留。
- 部署、备份、恢复、升级流程随真实使用收敛。

## 不纳入当前范围

- 记忆、知识库、研究综合、行动闭环、发布/订阅等任何内建产品逻辑。
- 重做独立 MIT 拆分版或追赶上游独立分支。
- 团队协作、云端同步、账号计费、企业管理后台。
- 双向办公写入、自动发布高风险操作、多 Runtime 调度。
- Windows/Linux 与商业分发；另行立项。