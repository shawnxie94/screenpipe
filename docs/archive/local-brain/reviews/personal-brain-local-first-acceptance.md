---
id: review-personal-brain-local-first-acceptance
type: review
status: partial
created_at: '2026-09-07'
updated_at: '2026-09-08'
related:
  - docs/plans/personal-brain-local-first-execution-plan.md
  - docs/prd/personal-brain-local-first.md
  - docs/trd/personal-brain-local-first.md
  - docs/reviews/personal-brain-brand-retained-identifiers.md
  - docs/roadmap.md
---

# Local Brain 首版实施验收记录（部分通过 + 具名残留）

> 结论先行：U01–U11、U13 全部代码落地，仓库各层测试绿，桌面开发版构建启动成功，对运行实例完成了全 REST 面实机走查，走查中发现的 5 个 bug 已修复并复验。U12 正式验收（≥10 真实会话、30–50 保留查询、两款工具 ≥90% 内容正确导入）依赖的外部授权件未齐，依赖真实样本的条目**未测、不记通过**。计划 status 维持 approved，未并入已完成基线。

## 1. 验证环境

- 分支 `zh-local`（含 462 个他人未提交的汉化改动文件，本次改动未创建 git 提交，避免混入）；base a255ffdf。
- 桌面开发版：`bun run dev:tauri`（debug-dev profile，native build queue），实例端口 3130，数据目录 `~/.screenpipe-dev/`，API 认证用本地 dev key（env 注入，取自本机 `screenpipe auth token`，仅本机 dev 实例使用）。
- 模型固定：Pi preset `shawnhub-copy` + `deepseek-v4-flash-0731`（严格绑定，无回落）。

## 2. 已实测通过项（真实证据）

### 2.1 测试基线

| 层 | 结果 |
|---|---|
| `screenpipe-db`（来源/修订/事务/迁移/删除/FTS） | 163 passed |
| `screenpipe-connect` office（固定 argv/分页/错误/身份） | 18 office + 既有全绿 |
| `screenpipe-engine` brain（Worker/REST/引用/取消） | 15 passed |
| `screenpipe-mcp`（answer/source 工具+旧契约） | 92 passed + typecheck |
| 前端（知识中心/连接面板/品牌） | 30 passed + typecheck |

### 2.2 实机走查（运行实例）

- **飞书真实链路**：连接状态→发现会话→版本解析→账号绑定（`auth status --json --verify`→identities.user）实测成功；真实消息同步被类型化失败拦截（见残留 ①）。
- **发布链路**：候选发布 v1 → 指针 CAS 陈旧返回 409 → v2 原子替代（同对象仅 1 条 published）。
- **反馈**：反馈定位到具体版本行 → 状态 pending → 恢复 resume 生效。
- **检索**：`POST /brain/search/reindex` 返回 `{"rebuilt":15}`，中文投影索引重建成功。
- **回填**：7 天显式回填入队（batch 2498 区间），任务进入 backfill_extract 队列。
- **无证据路径**：无可引用证据时返回 no_evidence，不编造。
- **严格绑定**：预设缺失时 `POST /answer` 返回 `model_binding_invalid`（含配置指引文案），不回落任何其他模型。
- **失败语义**：办公同步错误显示类型化 `capability_missing` + 精确缺失 scope 清单，而非裸 provider 输出。
- **删除边界**：未知来源操作返回 404；已删原文拒绝出站（EmissionGuard 单测覆盖）。

### 2.3 走查发现并修复的 5 个 bug（均已复验）

1. **BUG-1** Worker 启动早于 engine 共享状态，报 `shared state missing` → brain_runtime 轮询等待（60×500ms），已验证 worker 正常认领任务。
2. **BUG-2** lark-cli 错误 JSON 写 stderr 而 runner 丢弃，同步失败只显示 provider_error → stderr 全程透传+stdout 为空时回退读取，现显示精确缺失 scope。
3. **BUG-3** 知识列表标题显示"（无版本）" → 回退最新候选版本标题。
4. **BUG-4** `POST /answer` 被嵌套成 `/brain/answer` 致 404 → answer_route 顶层挂载。
5. **BUG-5** 反馈定位用空 knowledge_id+版本号查不到 → 改按版本行主键 id 定位，DTO 补 id 字段。

## 3. 具名残留（不阻塞代码交付，阻塞正式验收）

| # | 残留 | 影响 | 解除条件 |
|---|---|---|---|
| ① | lark-cli 用户身份缺 `im:chat:read`、`im:message.*` scope | 真实飞书消息导入未测（文档读取链路已验证） | 用户在飞书补授权后重跑连接刷新 |
| ② | `tmeet` 未安装（调研确认 v1.0.16） | 腾讯会议真实读取未测 | `brew install tmeet` 后设备码登录，重跑走查 U06 部分 |
| ③ | aiPresets（`~/.screenpipe-dev/store.bin`）未配置 `shawnhub-copy` 预设 | 真实模型问答/抽取未测（绑定错误路径已验证） | ~~设置中创建该预设~~ 2026-09-08 已自动化配置（key 取自钥匙串 kb-model `mlx-shaoxing-studio-openai`，端点 `http://100.104.58.62:8200/v1` 冒烟通过）；同日模型绑定按用户指令改为复用「模型与密钥」选中预设（见 §6），残留转为"真实模型回答待重建后重跑" |
| ④ | 走查触发的 ~2104 个 backfill_extract 任务在队列 | 预设缺失下会快速永久失败，无模型费用 | 已随 2026-09-08 dev 库重置消失；新实例需重新启用并回填 |
| ⑤ | dev 库留有走查样本 `walkthrough-sop-1`（标题含"走查样本"） | 测试数据残留 | 已随 2026-09-08 dev 库重置消失 |
| ⑥ | zh-local 既有 25 个前端 vitest 失败 | 与本次改动无关（已核对均为未触碰文件），属 462 个未提交汉化改动的既有状态 | 上游汉化工作自行收口 |
| ⑦ | UI AX/webview 树不可截取（窗口在副屏） | UI 走查以 REST 全量+组件测试+typecheck 替代，未做像素级目验 | 需要时把窗口移到主屏后可用 AX 工具补看 |

## 4. 未测项（如实保留，不计通过）

- U12 正式质量集：≥10 真实会话、≥3 同流程、30–50 保留查询、两款工具 ≥90% 内容正确导入、中文检索指标、resource/SLA、5 工作日集中审核观察——全部待残留 ①②③ 解除后执行。
- 腾讯会议分页/转写/纪要真实契约（fixture 已测，真机未测）。
- 重连/范围缩小/退出恢复的实机演练（单测+fixture 覆盖，真实授权样本未跑）。

## 5. 状态与后续

- 执行计划 `docs/plans/personal-brain-local-first-execution-plan.md`：status 维持 **approved**（按计划纪律"未完成不更新 completed"）；本记录即 U12 部分验收证据。
- roadmap「Local Brain 首版实施规划」条目已同步为"编码与实机走查完成、具名残留待外部授权件"。
- 未创建 git 提交、未推送。建议后续按模块分批提交（brain 契约/DB/engine/connect/Tauri/前端/文档各自成提交，中文+type 前缀，禁 `git add -A`），与 462 个既有未提交文件严格分离。

## 6. 2026-09-08 追记

- **模型绑定变更（用户指令，已实机验证）**：AI 能力改为复用「模型与密钥」中用户选中的预设（默认预设，缺省回退第一个），支持手动切换 Runtime，不再固定 `shawnhub-copy / deepseek-v4-flash-0731`、不再拒绝 ACP。实现：engine `ModelIdentity` 增加 `wire_api`、trait `identity()` 改为实时解析；`WorkerHandle` 持 executor 使 `/brain/status` 反映当前选择（实测显示 `pi / acp`）；Tauri 执行器完全镜像 chat 的预设→Pi 配置映射（含 ACP agent），非 ACP 补全仍禁工具并保留 brain 侧 token/上下文预算。engine brain 测试 16/16 绿。PRD/TRD/计划已加变更注记。
- **真实模型问答已通**：`POST /answer` 全链路（三路检索 sources 4/memories 2/knowledge 1 hits → 调用选中 ACP 预设 → 结构化 JSON）返回诚实 `partial` 回答 + 带 evidence_refs 的 claim + 来源摘录，无编造。
- **真实抽取已通**：7 天回填入队 2536 区间，抽取任务经 ACP agent 真实模型调用成功产出 WorkUnit（job 17502 succeeded，约 47s/个），compile 触发正常入队；worker 串行继续消化队列（旧区间快速 `no_evidence`/`input_deleted`，有文本区间才耗模型）。可用暂停接口中止。
- **当日修复 3 个新 bug**（均已实测验证）：
  1. `db/brain/history.rs` 5 处 SQL 字符串行尾误写 `\\`（字面反斜杠进 SQL → `unrecognized token`），`/brain/migration/*` 写路径在 09-07 最终重建后的二进制中全坏；改为单 `\` 续行后迁移/激活实测通过（brain enabled）。
  2. `create_backfill` 入队 payload 缺 `interval_start/interval_end`（只有 batch_id），backfill_extract 全部 `bad_payload` 永久失败；已补齐字段，实测任务真实执行。
  3. 抽取证据收集把「locator 行存在但无 OCR/转写文本」误判为 `input_deleted` 永久失败整个区间；改为「行不存在=已删除→拒绝；行在无文本→跳过该行」。同批增强 `parse_json_object`：严格解析失败时提取首个平衡 `{...}`（含字符串/转义感知），容忍 agent 式输出的散文/围栏包裹（新增 2 个单测）。
- **预设自动化（后被绑定变更取代）**：`shawnhub-copy` 预设已写入 dev store（key 来自钥匙串 kb-model `mlx-shaoxing-studio-openai`，全程本地；端点 `http://100.104.58.62:8200/v1` + `deepseek-v4-flash-0731` 真实冒烟通过）。绑定变更后该预设仅作为普通可选预设存在，用户可在「模型与密钥」中随时切回。
- **dev 库 brain 状态清零事件**：2026-09-08 上午实例重启后 brain 表曾全空（enabled:false、迁移 not_started），与昨晚日志（worker 持续运行至 08:03、job 8900+）矛盾；库文件 inode 与 09-07 18:30 隔离记录一致、迁移脚本无 DROP TABLE，清零机制未定死。当日随自动迁移恢复正常（history migration completed; brain enabled），采集帧数据（1.5GB）完好，`walkthrough-sop-1` v2 published 仍在。留待观察是否复发。
- **剩余路径**：lark-cli 补 `im` 读 scope、安装 tmeet 并登录后，跑 U12 真实样本验收（≥10 会话、30–50 保留查询、两款工具 ≥90% 内容导入），通过后计划并入已完成基线。

## 7. 2026-09-08 下午追记：授权到位 + 真实数据链路全通

- **两个外部授权件全部解除**：lark-cli 补齐 `im` 全套读 scope（`im:chat:read`、`im:message.group_msg/p2p_msg:get_as_user` 等，设备码流程）；tmeet v1.0.16 经 npm 安装并完成设备码登录（token 有效）。均实测验证。
- **飞书真实同步通过**：真实群「技术资讯」48h 窗口导入 2 条真实消息，连接状态 idle 无错误；消息内容进入检索并被真实 /answer 引用（诚实 partial：引用「李溪昂」「Infinitum 日报」「远程支持」等真实片段，明说证据不足，无编造）。
- **腾讯会议真实接入通过**：连接 supported + authorized（真实账号）+ 空录制列表正确处理（账号近 31 天无云录制——真实无样本状态，不是失败）。注意：record/transcript 深层命令的响应解析仍基于夹具契约，待账号出现真实云录制会议后才能实测（已记残留）。
- **手动切换 Runtime 实测生效**：默认预设从 `pi` 切到 `pi-MiniMax` 后 /brain/status 立即反映、下一次 /answer 即用新运行时且 200 成功（此前 `pi` 运行时超 45s SLA 连续 model_timeout——typed 可重试语义正确；MiniMax 快，一次通过）。本轮验证了切换设计的完整闭环。
- **当日再修 4 个 bug（累计 12）**，全部实机复验：
  9. 办公连接成功后 `last_error_code/message` 不清除——DB 更新按 `None=不改` 语义处理，加 `clear_errors` 标志显式清空。
  10. 回填/同步把窗口终点时间戳存成 cursor 并在下一轮当作 lark 不透明 `page_token` 回放 → `invalid page_token`；改为页 token 只在单轮内有效，整窗重走 + locator 去重。
  11. CLI 版本探针不剥 semver `v` 前缀，`tmeet --version` 输出 `v1.0.16` 被判 != `1.0.16` → unsupported；提取时 `trim_start_matches('v')`。
  12. tmeet 1.0.16 `auth status` 无 `--json` 旗标且只输出纯文本——argv 去掉 `--json`，解析器改为文本为主（Logged in/OpenId 行）+ JSON 兼容回退；meeting/record/transcript 全套 argv 与分页（`--start/--end ISO`、`--page-size`、`--page-token`、`--record-file-id`）对齐真实 CLI，transcript 段落单次返回无 pid 游标。
- **测试**：connect office 19、engine brain 16 绿。新增/更新 4 个夹具测试对齐真实契约。
- **U12 状态更新**：功能链路（含两款工具真实连接与飞书真实内容导入+问答引用）已全部真实走查通过；正式 37 项的量化门槛（≥10 真实会话、30–50 保留查询、腾讯会议 ≥90% 内容导入）需真实使用积累（尤其是有云录制的会议）后执行。

## 8. 2026-09-08 晚追记：对话接入知识库（MCP 注册）

- **问题**：连接页两款工具只把内容导入 brain，对话 agent 无工具可触达（sp_mcp_list_tools 空）。
- **修复**：把知迹 MCP（packages/screenpipe-mcp，含 answer/get-brain-source）以 stdio 注册进引擎 `/mcp-servers`（id `local-brain`，名「知迹知识库」，持久化于 `~/.screenpipe-dev/mcp_servers.json`）；dist 重新构建纳入新工具。注册 env 变量名须用 `SCREENPIPE_LOCAL_API_PORT`（api-base.ts 不读 `SCREENPIPE_PORT`）。
- **验证**：引擎 `/mcp-servers/local-brain/tools` 列出 30 个工具（answer/get-brain-source 在列）；`/mcp-servers/local-brain/call` 真实调用 answer 返回引用飞书「技术资讯」真实片段的诚实回答——与对话内 sp_mcp_call 同一通路。对话中即可发现并调用。
- **待产品化（记残留）**：注册目前是实例级手工步骤；产品化应在 brain 启用时幂等 upsert 该 MCP 条目（含打包路径/env 决策），另 engine 的 stdio launcher 已按配置传 env（.envs(env) 实测有效）。

## 9. 2026-09-08 深夜追记：对话改用原生工具，MCP 注册撤除

- **架构修正（用户判断）**：这 30 个工具是 screenpipe 自己的底层能力，对话属同一应用，正确形态是**原生内置工具**（Pi 扩展直调本地 REST），不应绕道 MCP stdio 子进程再回环 HTTP 调自己。MCP 包（packages/screenpipe-mcp）保留给外部 agent（Claude Desktop 等）作为知识出口。
- **落地**：新增 `assets/extensions/brain-tools.ts`（`brain_answer` + `brain_source` 两个收敛工具：带引用知识库问答、按 uid 取证据原文；对 typed 错误永不抛出、对重载荷做裁剪——claims≤8/sources≤6/摘录≤280 字符/原文≤4000 字符，防上下文膨胀），注册进共享扩展 seeding 清单（`SHARED_PI_EXTENSION_FILES` 平衡测试随行）；新增 5 项 vitest 全绿。
- **工具收敛结论**：原 30 工具与对话内置面无重复（对话仅 mcp 桥+应用扩展）；本轮按"知识问答"单一职责收敛为 2 个原生工具，检索/会议/录制/说话人/管道等能力保持原位（管道/对话侧另有扩展覆盖），后续有真实需求再按需加原生口。
- **撤除**：`local-brain` MCP 注册条目已 DELETE（mcp_servers.json 清空）；扩展文件在下次对话消息时自动种到聊天会话目录。
- **验证**：`/brain/sources/:id` 端点字段与工具输出对齐（SourceDetailDto.text，非 excerpt）；partial 形态（claims/sources 空、引用记忆）对工具健壮；parity 测试 shared_pi_extensions_cover_every_pi_harness 绿。
