---
id: plan-internal-skills-b04b
status: approved
owner: shawn
created: 2026-09-11
updated_at: 2026-09-11
plan_id: plan-internal-skills-b04b
plan_unit_id: root
base_commit: 9e64edd5c
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
  - D-16 第四条：skill 正文哈希进输入指纹
  - D-14 可复现要求：查询轨迹记录 + 查询上限强制执行（与 O-11 一并收口）
doc-covers: crates/screenpipe-db/src/db/knowledge, crates/screenpipe-db/src/migrations, crates/screenpipe-engine/src/knowledge, crates/screenpipe-engine/src/routes/activity_ledger.rs
doc-verified: 9e64edd5c
---

# 活动层收口 B04b：skill 哈希进输入指纹 + 查询轨迹与强制上限

## 1. 目标与非目标

### 1.1 目标（B04b）

1. **skill 正文哈希参与输入指纹**（D-16 第四条）：四个内部 skill 的正文哈希与提示词版本同等进入引擎输入指纹——skill 内容变化即触发重算/重编译，指纹成分可解释。
2. **查询轨迹与强制上限**（D-14 第四条、O-11）：取数读接口支持按调用方提供的 trace key 记录查询轨迹（method、path、参数、结果指纹），并按 trace key 强制查询上限；同一窗口重跑结果指纹一致或差异可解释（结果指纹只覆盖数据行，不含 `generated_at` 等易变信封字段）。

### 1.2 非目标

| 项 | 批次 |
| --- | --- |
| 把引擎内部提取改成"执行体自查"驱动（D-14 流程改造） | 后续阶段，保持引擎严格边界 |
| 前端、apps/** 任何改动 | 界面整理批 |
| RESERVED_SKILLS / 桌面导入保留名单 | 另行评估 |
| 对不带 trace key 的普通读请求做任何限制或记录 | 本批只对显式携带 trace key 的请求生效 |

## 2. 现状锚点

| 事实 | 位置 |
| --- | --- |
| 输入指纹核心（sources/logical_scope/分类/schema/prompt 五元） | `crates/screenpipe-db/src/db/knowledge/types.rs:422` `compute_input_hash` |
| 抽取/编译输入指纹入口 | `crates/screenpipe-engine/src/knowledge/sources.rs:268` `input_hash_for`（调用方 `extract.rs:188`、`compile.rs:162`） |
| 摘要输入指纹（含证据对与模型身份） | `crates/screenpipe-engine/src/knowledge/summarize.rs:461` `summary_input_hash`；enqueue 侧 `summarize.rs:84` |
| 抽取提示词版本 | `sources.rs:22` `EXTRACT_PROMPT_VERSION = "zh-extract-v2"` |
| 活动读接口（B01） | `crates/screenpipe-engine/src/routes/activity_ledger.rs`（`/activity-intervals`、`missing-summary`、`:id/evidence`） |
| skill 引用的其余读接口 | `/search`（server.rs:716）、`/frames/:id/text`（739）、`/frames/:id/context`（743）、知识域 nest（1141–1145） |
| 引擎侧引用 skill 正文的既有模式 | `crates/screenpipe-engine/src/cli/agent.rs` `include_str!("../../../screenpipe-core/assets/skills/...")` |
| 最近迁移号 | `20260911150000_knowledge_jobs_allow_summarize.sql` |
| skill 正文行数上限与品牌中性测试 | `crates/screenpipe-core/src/agents/pi.rs`（≤120 行、品牌零命中） |
| skill 接口真实性回归 | `crates/screenpipe-engine/tests/internal_skill_routes.rs`（nest 归属 + method 校验 + 8 负例） |

## 3. 交付物

### 3.1 skill 哈希进输入指纹

1. `compute_input_hash` 增加 `skill_revision: &str` 成分（追加进既有版本元组；保持既有调用语义可测）。
2. `input_hash_for` 透传 `skill_revision`：`extract.rs` 传 `work-unit` skill 哈希；`compile.rs` 传 `knowledge-distill` skill 哈希。
3. `summary_input_hash` 与 enqueue 侧哈希追加 `activity-summary` + `knowledge-fetch` skill 哈希成分（两处保持同一口径）。
4. skill 哈希 = `sha256(include_str! 正文)` 前 16 hex；正文来源用与 `cli/agent.rs` 相同的相对 `include_str!`，不引入运行时文件读取。
5. 单测：skill 文本变化 → 输入指纹变化；skill 哈希成分出现在指纹输入说明中（沿用 `fingerprint_inputs` 说明字段惯例时同步更新）。

### 3.2 查询轨迹与强制上限

1. 新迁移 `20260911160000_knowledge_query_trace.sql`：表 `knowledge_query_trace`（`id`、`trace_key`、`method`、`path`、`query_string`、`result_fingerprint`、`row_count`、`status_code`、`created_at`；索引 `(trace_key, id)`）。不改任何已应用迁移。
2. DB API：`knowledge_record_query_trace(...)`、`knowledge_query_trace_count(trace_key) -> i64`（含单测）。
3. 引擎侧在 §2 所列 skill 引用的读接口上支持可选 `X-Screenpipe-Trace` 头：
   - 带 header：成功响应后记录一条轨迹；**结果指纹只对数据行序列化计算**（如 intervals/evidence/work_units 列表），排除 `generated_at`/`time_range` 信封；同一窗口重跑两次指纹一致。
   - 强制上限：同一 `trace_key` 生命周期记录数上限 200，超出返回 429 与说明错误体；上限检查在处理前执行。
   - 不带 header：行为与现在完全一致（不记录、不限流）。
   - 轨迹写入失败不吞读失败：返回原读结果同时记录错误日志。
4. `/knowledge/knowledge`、`/knowledge/work-units[/:id]`、`/knowledge/status` 同样支持（知识域 nest 内逐 handler 或统一 helper）。
5. 测试（engine，临时库直调 handler 或既有测试 server 模式）：同窗口两次同查询 → result_fingerprint 相同；不同窗口 → 不同；超过 200 → 429；无 header → 表中无记录。

### 3.3 skill 正文与守卫同步

1. `knowledge-fetch` 增补一小节：`X-Screenpipe-Trace` 头（每任务一个 key、上限 200、429 语义、轨迹记录内容），保持 ≤120 行、品牌中性。
2. 其余三个 skill 不改（取数统一委托 knowledge-fetch）。
3. `internal_skill_routes` 若正文新增 `METHOD /path` 引用须全部真实（预期只加 header 描述，不加新路径）；既有 8 条负例保持通过。

## 4. 测试与验收命令

| # | 命令 | 期望 |
| --- | --- | --- |
| 1 | `cargo test -p screenpipe-db --lib knowledge` | 全绿（含迁移与 trace DB 单测、input-hash 新单测） |
| 2 | `cargo test -p screenpipe-db --lib activity` | 全绿（不回归） |
| 3 | `cargo test -p screenpipe-engine --lib knowledge` | 全绿（input-hash 透传 + trace handler 测试） |
| 4 | `cargo test -p screenpipe-engine --lib activity_ledger` | 全绿 |
| 5 | `cargo test -p screenpipe-engine --test internal_skill_routes` | 全绿（8 负例保持） |
| 6 | `cargo test -p screenpipe-core --lib skills` 与 `--lib pi` | 全绿（行数上限、品牌中性、基线注入不回归） |
| 7 | `cargo check -p screenpipe-core -p screenpipe-engine -p screenpipe-db` | exit 0 |
| 8 | `cargo test -p screenpipe-db --test knowledge_correctness` | 全绿（compute_input_hash 签名变化的调用点同步） |
| 9 | 静态核对：报告附「指纹成分表」与「轨迹覆盖接口表」 | 每个成分、每个接口都能在代码定位 |

## 5. 风险与回退

| 风险 | 处置 |
| --- | --- |
| input_hash 语义变化触发存量重算 | 正是 D-16 预期（skill 版本变化即重算）；报告说明影响面；不迁移旧哈希 |
| trace 表无限增长 | 上限 200/trace_key 天然有界；`knowledge_query_trace_count` 索引查询；报告给出后续清理建议（本批不做定时清理） |
| 大响应指纹成本 | 只对数据行序列化计算，行数本身有接口既有 limit 有界 |
| header 信任问题 | 本地单人工具，非对抗场景；文档明确 header 语义 |

**回退**：删除迁移文件与 trace 代码、还原 `compute_input_hash` 签名即可；无数据破坏。

## 6. 交付边界

- 允许路径：`crates/screenpipe-db/src/migrations/20260911160000_knowledge_query_trace.sql`、`crates/screenpipe-db/src/db/knowledge/**`、`crates/screenpipe-db/src/db/mod.rs`（如需注册）、`crates/screenpipe-db/tests/knowledge_correctness.rs`（仅机械签名同步）、`crates/screenpipe-engine/src/knowledge/**`、`crates/screenpipe-engine/src/routes/activity_ledger.rs`、`crates/screenpipe-engine/src/routes/frames.rs`（如 `/frames` handler 在此）、`crates/screenpipe-engine/src/server.rs`（仅 `/search` handler 挂载点就近修改，禁改路由表结构）、`crates/screenpipe-core/assets/skills/knowledge-fetch/SKILL.md`；报告限 `.agent/tmp/internal-skills-b04b-report.md` 与 `.agent/tmp/b04b/`。
- 禁止：改已应用迁移、`apps/**`、`docs/**`、`crates/screenpipe-engine/src/agent_skills.rs`、用户技能 store、前端；不做 D-14 流程改造；不加新依赖。
- 不 commit / 不 push；不得真调远程模型；真实库只读。
- 单一 root、单一写者；子任务期间协调者不改 HEAD。
