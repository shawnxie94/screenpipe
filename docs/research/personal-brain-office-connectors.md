---
id: research-personal-brain-office-connectors
type: research
status: researched
created_at: 2026-09-07
updated_at: 2026-09-07
related:
  - docs/prd/personal-brain-local-first.md
  - docs/trd/personal-brain-local-first.md
  - docs/research/personal-brain-local-first-preflight.md
  - docs/roadmap.md
---

# Local Brain：办公 CLI / MCP 接入调研

> 后续范围更新（2026-09-07）：用户已确定首版只接入飞书和腾讯会议，操作归入「连接」，WPS 后置；跳过下文建议的规划前 PoC，直接生成实施计划。下文保留调研时的证据与候选比较，原“三入口验证”不再是当前待办。现行范围见 PRD §13.9 和实施计划。

> 研究结论与待验证建议，不是连接器验收或范围冻结。当前 PRD R9 / TRD 仍将办公云 API 后置；本文建议重新评估部分官方只读接口的优先级，不能据此宣布现有办公采集阻塞已解决。国内 Runtime 兼容继续后置。

## 1. 结论

建议采用“结构化内容优先，Screenpipe 记录实际操作并补足不可读内容”的组合。飞书优先复用已安装的官方 `lark-cli`；腾讯会议优先验证官方 `tmeet`；WPS 云文档验证官方云端 MCP，本机文档另走文件读取或采集路径。上一版把办公 API 统一后置，遗漏了已经存在的官方工具，应修正这一选型判断。

这能减少聊天、文档和会议转写的 UI 解析工作，也更容易保留对象 ID、时间、版本和来源链接。但它不会自动解决账号授权、增量读取、删除失效、重复证据和离线可用性。这些是 Local Brain 自己仍须处理的边界。

本文重点核对六个问题：是否官方维护；macOS 是否适用；能读什么内容；账号与权限限制是什么；是否有隐藏写操作；如何验证实际收益。

## 2. 候选与选择

| 对象 | 已确认的工具 | 能力与限制 | 建议 |
|---|---|---|---|
| 飞书 | 官方 [larksuite/cli](https://github.com/larksuite/cli)，本机 `lark-cli 1.0.65` | 文档读取/搜索、消息搜索及元数据读取；内容范围取决于用户身份与授权 | 第一顺位，只封装所需只读命令 |
| 腾讯会议 | 官方 [TencentCloud/tencentmeeting-cli](https://github.com/TencentCloud/tencentmeeting-cli)，命令 `tmeet`，MIT | 会议记录、录制列表、分段转写、智能纪要；需已有可访问的云端内容 | 优先 CLI，先验证一场本人可访问的会议 |
| WPS 365 云文档 | 官方 [WPS MCP Server](https://open.wps.cn/documents/app-integration-dev/mcp-server/introduction) | 云端检索、正文提取、文件信息；应用授权与权限配置有成本 | 有对应账号与权限时优先官方方案 |
| WPS / 金山文档 SkillHub | WPS 域名下 [SkillHub MCP](https://mcp-center.wps.cn/skill_hub/mcp) | 本轮发现文件搜索、正文读取、版本查询工具；账号认证和实际文件可读性未验证 | 作为个人云文档入口候选，先验证认证，不能仅凭工具列表宣布可用 |
| 本机 WPS，macOS | 社区 [lc2panda/wps-skills](https://github.com/lc2panda/wps-skills)，MIT | 用 WPS 加载项与本地桥接服务访问已打开文档，需要额外安装 | 保留为 PoC 候选，先比较文件读取与 AX/OCR 是否已够用 |
| 本机 WPS，Windows | 社区 [jjchen17/wps-cli](https://github.com/jjchen17/wps-cli)，MIT | 依赖 Windows COM，不适用于本机 macOS | 本轮不选 |

“官方、有活跃仓库、文档齐全”足以支持优先试用，不能替代本项目的兼容性和生命周期验收。社区工具也不能仅按工具数量判断成熟度。

### 2.1 飞书：现成 CLI 可直接复用，消息授权还有缺口

本机只读检查结果：

- 路径 `/opt/homebrew/bin/lark-cli`，版本 `1.0.65`；查看了本机帮助和随 CLI 分发的技能说明。
- `docs +fetch` / `docs +search` 可作为文档入口；当前读取格式不能一律假定为 Markdown，需按真实输出解析。
- 本机 `im +messages-search` 为用户身份能力，支持会话、发送者、时间等条件，页大小 1–50；不要用最新网站描述覆盖已安装版本的实际行为。
- `auth status --json --verify` 返回 `verified: true`，同时用户/token 状态为 `needs_refresh`。这不等于已经验证了正文读取成功。
- `auth check` 确认已有 `docx:document:readonly`、`search:docs:read`；缺少 `search:message`。本轮没有启动补授权，没有读取真实聊天或文档正文。

建议首个样本为“一个明确文档 + 一个明确会话的短时间窗口”，不要从全账号历史同步开始。消息、文档 ID 与来源 URL 用于精确回链；编辑、撤回、分页完整性仍须单独验证。[官方 CLI](https://github.com/larksuite/cli)、[消息搜索说明](https://github.com/larksuite/cli/blob/main/skills/lark-im/references/lark-im-messages-search.md)

### 2.2 腾讯会议：官方 CLI 已具备转写读取能力

官方仓库的最新变更记录为 `v1.0.16`（2026-09-02），本机尚未安装 `tmeet`。采用 OAuth 设备码登录，默认输出 JSON；已有 `record list`、`record transcript-get`、`record transcript-paragraphs`、`record transcript-search`、`record smart-minutes` 等命令。列表分页与转写的 `pid/limit` 不是同一种游标，应按各命令契约处理。[仓库](https://github.com/TencentCloud/tencentmeeting-cli)、[变更记录](https://github.com/TencentCloud/tencentmeeting-cli/blob/main/CHANGELOG.md)

账号资格要按入口分别核对：官方 CLI 帮助称个人版、专业版开放使用，商业版、企业版需申请；MCP 帮助仍有不同的内测限制，不能混为一个统一兼容承诺。CLI 文档描述了加密与系统钥匙串保护，仍需在本机接入时核实实际凭据落点。[CLI 使用说明](https://meeting.tencent.com/support/topic/2236/index.html)、[MCP 使用说明](https://meeting.tencent.com/support/topic/2233/index.html)

官方 MCP 也提供录制、转写和智能纪要工具，但依赖主持人开启云录制、处理完成以及访问权限；文档给出的处理时间为 5–30 分钟。没有对应云端转写时，接口不会补出不存在的内容，仍需要 Screenpipe 本地音频路径。智能纪要是模型生成内容，应标明来源，并优先引用带时间位置的原始转写。[MCP 能力与限制](https://meeting.tencent.com/support/topic/2233/index.html)

### 2.3 WPS：云端官方路径与桌面路径分开验证

**WPS 365：文档化的官方 MCP。** 官方变更记录包含 OAuth、Streamable HTTP 和 2026-01-08 的 MCP 2.0 更新。云文档工具包括检索、Markdown 正文提取和文件信息读取；适合已有云端资源和相应权限的场景。[更新记录](https://open.wps.cn/documents/app-integration-dev/mcp-server/change-log)、[云文档工具](https://open.wps.cn/documents/app-integration-dev/mcp-server/tools/mcp_kso-yundoc)

需要注意，云文档 MCP 的公开权限标记为 `delegated:kso.mcp_yundoc.readwrite`，同组也有共享/授权类写工具。使用“只读连接器”必须由本地代码限定工具和参数，不能把供应商授权范围描述成只读。[权限与工具列表](https://open.wps.cn/documents/app-integration-dev/mcp-server/tools/mcp_kso-yundoc)

**SkillHub：服务可发现，账号可用性仍待测。** 本轮对 WPS 域名端点只做 MCP 初始化与 `tools/list`，服务报告 `SkillHub MCP / 1.0.0`，返回 561 个工具。发现 `search_files`、`get_file_info`、`list_file_versions`、`read_file`、`read_file_content`。这是公开 schema 的实测，未提供用户凭据，也没有执行内容读取；不能推出个人账号已获授权或服务具备正式支持承诺。[实测端点](https://mcp-center.wps.cn/skill_hub/mcp)

公开 schema 还揭示两项需要落在适配代码里的限制：

- `read_file` 的媒体上传选项默认 `true`，`read_file_content` 对应选项默认 `false`；文本采样应显式传 `enable_upload_medias=false`，避免依赖不同工具的默认值。正文抽取可能返回异步任务，需限制轮询与总时长。
- `wps.texts.content` 同时支持 `query/insert/update/delete`。如确需使用它，必须只允许 `verb=query`；只限制工具名不足以保证只读。

这些事实来自本轮公开 schema，不是已完成的产品行为测试。原始 schema 临时保存在 `/tmp/screenpipe-office-connector-research/kdocs-tools.json`，不作为永久依赖；正式 PoC 应重新发现并保存不含账号信息的必要契约。

**本机 WPS：尚未确认开箱即用且成熟的 macOS CLI。** `wps-skills` 的 macOS 路径使用加载项和 HTTP 轮询桥接，能作为本机文档候选，但会增加安装和维护成本。另一个社区项目 `WPS-AI` 提供 macOS 版本，不过它本身是完整 AI 助手，范围较大，且 README 列有 WPS 26884 及以上版本的兼容问题，不优先引入其整套 AI 栈。[wps-skills 安装](https://github.com/lc2panda/wps-skills/blob/main/INSTALL.md)、[WPS-AI](https://github.com/lewis-hui1202/WPS-AI)

对于用户明确选择的已保存文件，可先验证普通文件解析是否足以取正文；未保存编辑、正在操作的单元格/页面以及实际操作过程，保留本机采集或加载项路径。云端读取结果不能覆盖同一文档尚未同步的本机状态。

## 3. 对 Local Brain 技术方案的建议

这是待纳入 PRD/TRD 的设计输入，不是现有接口已经支持的功能。

1. **连接器负责取内容，采集负责记录活动。** 内容经统一归一化进入现有证据、索引和删除链路，不另建知识真源。比如有权读取某条聊天，只能说明内容可访问；必须另有活动证据，才能声称用户当时看过或处理过它。
2. **建立稳定来源身份与版本。** 至少区分供应商、账号、对象 ID、对象版本/内容摘要、发生时间、抓取时间、读取方式和来源链接。没有版本号时显式使用内容摘要，不伪造供应商 revision。API 与截图可关联到同一对象，但保留各自观察时间与原始来源。
3. **范围由用户选择的资源和工作上下文限定。** 首轮只读取明确文档、会话或会议；按需补正文，后续再判断是否需要定时增量。公司活文档真源仍在云端，不将正文复制到 git 或个人文档知识库；Local Brain 内容缓存必须服从账号、资源授权和保留设置，不能从“工具有权限”推导“全部内容都可沉淀”。
4. **读取行为由程序限制。** CLI 使用固定可执行文件与 argv、固定版本、JSON 校验、超时和输出大小上限，不拼 shell。MCP 只注册实际需要的读取能力，并校验混合工具的参数；文档正文作为不可信数据，不能触发新工具调用。模型抽取仍复用现有 Pi/Preset，不加载整个办公操作工具集。
5. **复用现有证据生命周期。** 删除本地来源需阻止连接器再次导入，并让派生知识失效；不调用远端删除。权限撤销、文档更新、转写尚未就绪、分页不完整、限流和认证失效分别建模，不能统一伪装成“没有内容”。离线缓存标注抓取时间，不能声称实时同步。
6. **CLI/MCP 两种协议按工具选择即可。** 本轮只统一最小的读取结果与错误，不自研通用连接器平台。读取办公 MCP 是内容入口；对外提供 Local Brain MCP 是知识出口。两者都不要求重新支持国内 Agent Runtime。

预期收益主要是正文完整度、稳定字段与较少的客户端 UI 适配；是否整体更省工，还取决于真实账号资格、可读范围和授权成本。应先通过下列样本再确定首版交付承诺，不给未经验证的工期折扣。

## 4. 最小验证与范围收口

| 验证 | 最小样本 | 判定依据 |
|---|---|---|
| 飞书 CLI | 一个明确文档、一个明确会话的短窗口；消息搜索需补授权 | 正文、ID、发送者/时间、分页和回链可核对；授权失效可识别 |
| 腾讯会议 CLI | 一场本人可访问且已有云端转写的会议 | 原始转写及时间位置可读；纪要与原始转写分开；无录制/无权限状态正确 |
| WPS 云端 MCP | 一个本人可访问的云文档 | 认证方式、正文与版本可用；无共享修改或媒体上传；异步抽取可停止 |
| WPS 本机与采集兜底 | 一个合成的已保存文档及一次未保存修改 | 文件/云端/可见状态的差异可识别；不会把旧正文作为最新内容 |
| 共用生命周期 | 同一资源重复读取、一次编辑、本地删除后再次发现 | 幂等、修订、删除屏障、引用失效都正确；真实内容不进入诊断日志 |

这些是选型 PoC，不替代 PRD 规定的正式质量集与三款办公工具验收。CLI/MCP 成功也不能冒充 AX/OCR 或本地音频通过。若将结构化读取前置，需要同步修改 PRD R9、TRD 的边界/来源类型/身份与删除规则/兼容矩阵及评审依据，再冻结执行计划；应用内指定 Pi 模型绑定与取消清理的验证仍独立存在。

建议顺序：先飞书，再腾讯会议，接着 WPS 云端；仅在本机文档仍有明显缺口时试装 WPS 加载项。首版候选范围限于三个必要的只读入口；办公写操作、全账号历史同步、通用多租户连接器平台与国内 Runtime 兼容仍后置。

## 5. 本轮验证边界

已完成：官方文档与项目维护信息核对、本机飞书 CLI 版本/帮助/权限检查、WPS SkillHub 公开协议及工具 schema 发现。未安装 `tmeet` 或 WPS 加载项，未新增账号授权，未读取或外发真实办公正文，没有形成连接器端到端通过结论。

本记录只包含公开技术信息和脱敏状态。临时下载的公开文档和协议 schema 不进入应用配置，不携带账号、令牌或模型密钥。
