---
id: plan-internal-skills-b04a
status: approved
owner: shawn
created: 2026-09-11
updated_at: 2026-09-11
plan_id: plan-internal-skills-b04a
plan_unit_id: root
base_commit: 0c2ccc796385b2238974eda7df971e0291791bbf
orchestration_mode: batch
execution_target: subagent
execution_backend: pi_subagent
runtime_adapter: zcode
logical_role: worker
subagent_role: zcode
subagent_scope: user
selected_subagent_model: builtin:bigmodel-coding-plan/GLM-5.3-Flash
thinking: low
fallback: false
parallel_mode: serial_same_worktree
acceptance_scope: batch
framework: docs/trd/personal-workbench-integration-framework.md#d-14
covers:
  - D-14 取数逻辑沉淀为内部 skill（查什么/顺序/时间对齐/去重/上限），铁律留在系统指令
  - D-16 真源中性 + 按 Agent 注入；四个 skill 全部实现
doc-covers: crates/screenpipe-core/assets/skills, crates/screenpipe-core/src/agents, crates/screenpipe-engine/src/cli/agent.rs
doc-verified: 9c7b8bff2
---

# 活动层收口 B04a：四个内部 skill（共享取数 + 三个任务）执行计划

## 1. 目标与非目标

### 1.1 目标（B04a）

把「怎么取数、怎么产出」沉淀成四个随应用交付的内部 skill，正文只存一处（中性资源目录）、无任何 Agent 品牌，并按现有 Pi/ACP 注入层与 `AgentLayout` 表投递；skill 正文里的每个接口都必须真实存在于当前路由表。

### 1.2 非目标

| 项 | 批次 |
| --- | --- |
| skill 正文哈希进输入指纹（D-16 第四条） | B04b |
| 查询轨迹记录与查询上限的强制执行（D-14 可复现要求） | B04b（与 O-11 一并收口） |
| 把引擎内部提取改成"执行体自查"驱动（D-14 的流程改造） | 后续阶段（当前引擎严格边界保持不变） |
| 前端「N 个会话」文案跟进 | 界面整理批 |

## 2. 现状锚点

| 事实 | 位置 |
| --- | --- |
| 内置技能资源（中性目录） | `crates/screenpipe-core/assets/skills/{screenpipe-api,screenpipe-cli,screenpipe-chats,render-html-report}/SKILL.md` |
| Pi 注入：基线技能集合 + 会话目录物化 | `crates/screenpipe-core/src/agents/pi.rs`（`ensure_screenpipe_skill`、`render_screenpipe_api_skill`） |
| 用户技能 store（与内置分离、带来源指纹标记） | `crates/screenpipe-core/src/agents/pi.rs::sync_user_skills_from`（`<data_dir>/skills`） |
| 按 Agent 布局（skills_dir / mcp_path / mcp_format） | `crates/screenpipe-engine/src/cli/agent.rs`（`AgentLayout`、`install_skills`） |
| 活动层读接口（B01 挂载） | `GET /activity-intervals`、`/activity-intervals/missing-summary`、`/activity-intervals/:interval_id/evidence` |
| 知识域路由 | `crates/screenpipe-engine/src/routes/knowledge*.rs`、`crates/screenpipe-engine/src/knowledge/routes.rs` |
| 摘要/工作单元规则真源 | 引擎提示词（`knowledge/prompts/mod.rs`）与 `registry/mod.rs` 校验 |

## 3. 交付物：四个 skill

正文目录：`crates/screenpipe-core/assets/skills/<name>/SKILL.md`（中性资源目录，D-16 已满足）。

| skill | 内容要点 |
| --- | --- |
| `knowledge-fetch`（共享取数） | 只读取数配方：先读间隔与摘要（时间范围 → `GET /activity-intervals`），再按需回捞原始证据（`/activity-intervals/:id/evidence`）或检索知识；写清**顺序、时间对齐（区间左闭右开、时区 RFC3339）、去重口径、每条上限与总量上限、证据引用格式**；必须写明「只读、不得写库、不得调用非本地接口」 |
| `activity-summary`（活动总结） | 给定时间范围如何产出间隔摘要：取数走 `knowledge-fetch`；输出契约（自足、分档 <15min 80–150 字 / 15–60min 150–300 / >60min 300–600、关键字 5–12 含专名、1–3 条引证）；措辞不得超出证据（采样记账）|
| `work-unit`（工作单元） | 从摘要开始（先读摘要与关键字），按需回捞细节；输出 schema v2（含 process/environment/details）；强制证据引用；办公资料只是资料 |
| `knowledge-distill`（知识提炼） | 只从 WorkUnit 提炼；SOP 至少 3 个**互不相同的活动**、DecisionRule/ExceptionPlaybook 需 `single_observation=true` 与边界；证据不足宁可少产出 |

规则：

1. **正文不得出现 Agent 品牌名**（Claude / Codex / Gemini / Cursor / Pi 等），只描述"本地知识服务"与接口；测试断言品牌词表零命中。
2. **接口必须真实存在**：正文中出现的每个 `METHOD /path` 都必须能在 `crates/screenpipe-engine/src/server.rs` 的路由表里找到；交付报告里附「skill 接口 → server.rs 行号」对照表。
3. 正文同时给出**铁律与取数的分工**：skill 只放取数方法与输出契约；不得把安全铁律写成"可选建议"。
4. 与引擎提示词口径一致（分档字数、会话门槛、v2 字段名）；不一致处按提示词为准并在报告里记录。

## 4. 注入

1. **Pi**：`ensure_screenpipe_skill` 的基线集合加入四个 skill（与 `screenpipe-api` 等同等对待），保证每个会话都能加载。
2. **按 Agent 布局**：如 `install_skills`/`AgentLayout` 有既有投递路径，四个 skill 随内置集合一起投递；**不新增写死路径**。
3. **用户技能 store 不受影响**：不得写入 `<data_dir>/skills`（那是用户导入区，带来源指纹标记），内置技能仍物化到会话技能目录。

## 5. 测试与验收命令

| # | 命令 | 期望 |
| --- | --- | --- |
| 1 | `cargo test -p screenpipe-core --lib skills` | 新增：四个 SKILL.md 存在且非空、品牌词表零命中、front-matter（name/description）齐备 |
| 2 | `cargo test -p screenpipe-core --lib pi` | 基线集合包含四个新 skill；注入后会话技能目录内含 `SKILL.md` |
| 3 | `cargo test -p screenpipe-engine --lib agent` | `install_skills` 相关用例全绿（若涉及内置集合断言，按新增四个更新） |
| 4 | 静态核对：报告附「skill 接口 → `server.rs` 路由行」对照表 | 每个接口都能在路由表定位 |
| 5 | `cargo check -p screenpipe-core -p screenpipe-engine` | exit 0 |
| 6 | `cargo test -p screenpipe-engine --lib knowledge` | 全绿（提示词未改） |
| 7 | `cargo test -p screenpipe-engine --test internal_skill_routes` | 正例全绿；必须拒绝未挂载 `/knowledge`、`/knowledge/:id`、虚构 `/knowledge/activity-intervals`，及错误 HTTP method |

## 5.1 独立复核后的有界纠偏（同一 B04a，不重做已交付实现）

按以下顺序串行执行，全部完成才交回：

1. 保留已修正的三处知识接口引用（完整路径为 `/knowledge/knowledge` 与 `/knowledge/knowledge/:id`），替换协调者新增的过宽路由测试：按实际 nest 归属及 HTTP method 验证。禁止把所有子路由当顶层，也禁止 prefix × literal 任意组合；回归测试包含 §5 第 7 项负例。可用限定语法的 fail-closed 源码校验或复用实际 Router 的测试；不改生产路由，也不新增通用路由解析框架/依赖。
2. 修正 `knowledge-fetch`：evidence 接口只有 `limit`（1–1000），无 offset/cursor；`truncated` 只表示返回条数触达 limit，不保证还有一条。提高 limit 仍触顶时报告缺口，不教虚构分页。
3. 引用编号在整个任务唯一；建立 sN→interval_id、eN→interval_id/source_type/source_id、uN→WorkUnit id 映射，跨段不重用裸 eN。其余三个 skill 同步口径，保留 ≤120 行限制。
4. 核对知识提炼 JSON 与引擎实际 prompt/registry，尤其 ExceptionPlaybook 的 boundary 表达；以现有代码为准，报告差异，不为迎合文档改变产品 schema。
5. 执行 §5 全部检查；交付报告记录命令、退出码、路由完整挂载位置和受影响正文规则。所有 source/测试改动交回协调者独立验收。

验收 ID：`skills`、`pi`、`agent`、`route-map`、`check`、`knowledge`、`internal-skill-routes`。
证据：`.agent/tmp/b04a-independent-review/feedback.md` 与 `route-negative-probe.log`；新报告 `.agent/tmp/internal-skills-b04a-repair-report.md`。

运行时说明：宿主是 Pi，调用原生 `subagent` 工具，故 backend 为 `pi_subagent`；工具更新后 `role=zcode` 显式选择 ZCode 外部运行时。用户已明确授权当前 GLM-5.3-Flash/low 配置，不再受之前的 gpt-5.6-luna 请求阻塞。调度及验收归协调者，执行者禁止递归委派。

## 6. 风险与回退

| 风险 | 处置 |
| --- | --- |
| skill 正文写错接口 → 执行体按错误接口取数 | 交付报告强制对照表；我方逐条核对路由 |
| 内置技能进入用户 store 造成"用户技能"污染 | 明确规定不写 store；测试断言 store 不被写入 |
| 正文与引擎提示词口径漂移 | 口径以提示词为准；报告记录差异 |
| 四个 skill 让每个会话上下文变长 | 正文短小（每个 ≤120 行），只讲取数与契约 |

**回退**：删除四个资源目录与注入列表即可。

## 7. 交付边界

- 允许路径：`crates/screenpipe-core/assets/skills/**`、`crates/screenpipe-core/src/agents/**`、`crates/screenpipe-engine/src/cli/agent.rs`、`crates/screenpipe-engine/tests/internal_skill_routes.rs`；报告限 `.agent/tmp/internal-skills-b04a-repair-report.md` 与 `.agent/tmp/b04a-repair/`。
- 当前纠偏不改上述清单以外代码；`RESERVED_SKILLS` 与桌面导入保留名单后续另行评估，不以此越界。既有 `docs/archive/` 为无关未跟踪内容，保留不动。
- 单一 root/单一写者、无并行任务；调用前锁定计划哈希与 dirty 文件快照，协调者在子任务 terminal 前不改 HEAD、不写共享源码。
- 禁止：改 `crates/screenpipe-engine/src/knowledge/**` 的提示词与校验、前端、`docs/**`、已存在迁移；不写 `<data_dir>/skills`；不做 B04b 的哈希进指纹与轨迹记录。
- 不 commit / 不 push；不得真调远程模型。
