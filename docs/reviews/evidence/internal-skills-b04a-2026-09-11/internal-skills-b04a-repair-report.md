# B04a 有界纠偏交付报告（attempt=1）

- 任务包：.agent/tmp/b04a-repair/task-pack.yaml（b04a-repair，phase build）
- 工作区：/Users/shawn/Developer/GitHub/products/screenpipe，base commit 0c2ccc796
- 执行：ZCode worker（builtin:bigmodel-coding-plan/GLM-5.3-Flash, thinking low），未递归委派
- 结论：**completed** —— 纠偏四步全部完成，6/6 条验收命令 exit 0，diff 只落在 allowed_paths

## 1. 实际读取/修改的文件

本轮修改（全部在 allowed_paths 内）：

| 文件 | 改动 |
| --- | --- |
| crates/screenpipe-engine/tests/internal_skill_routes.rs | 整体重写：真实 nest 归属 + HTTP method 验证 + 8 条回归负例；禁全局笛卡尔积 |
| crates/screenpipe-core/assets/skills/knowledge-fetch/SKILL.md | 分页纠偏 + 引用编号任务内唯一化（85→100 行） |
| crates/screenpipe-core/assets/skills/activity-summary/SKILL.md | 引用编号口径同步（56→58 行） |
| crates/screenpipe-core/assets/skills/work-unit/SKILL.md | 引用编号口径同步（57→59 行） |
| crates/screenpipe-core/assets/skills/knowledge-distill/SKILL.md | JSON 对齐引擎 registry + uN 映射口径（56→67 行） |

读取（事实核对）：server.rs、knowledge/routes.rs、knowledge/migration.rs、knowledge/answer.rs、knowledge/registry/mod.rs、knowledge/prompts/mod.rs、knowledge/compile.rs、routes/activity_ledger.rs、screenpipe-core/src/agents/pi.rs（测试段）、复核证据 feedback.md 与 route-negative-probe.log。

pi.rs 与 cli/agent.rs 本轮**未改动**（预先存在的 B04a dirty diff 原样保留，无需变更）；docs/** 未触碰。

## 2. 执行的命令及结果（日志均在 .agent/tmp/b04a-repair/）

| 命令 | 退出码 | 日志 | 结果 |
| --- | --- | --- | --- |
| cargo test -p screenpipe-core --lib skills | 0 | test-core-skills.log | 4 passed |
| cargo test -p screenpipe-core --lib pi | 0 | test-core-pi.log | 405 passed |
| cargo test -p screenpipe-engine --lib agent | 0 | test-engine-agent.log | 46 passed |
| cargo test -p screenpipe-engine --test internal_skill_routes | 0 | test-engine-internal-skill-routes.log | 2 passed |
| cargo check -p screenpipe-core -p screenpipe-engine | 0 | check-core-engine.log | Finished |
| cargo test -p screenpipe-engine --lib knowledge | 0 | test-engine-knowledge.log | 46 passed |

反向探针（证明新测试真的拒绝坏路径，不再是误报通过）：临时在 knowledge-fetch/SKILL.md 写入 `GET /knowledge`、`GET /knowledge/activity-intervals` 后，`internal_skill_endpoints_exist_in_the_engine_router` FAIL 并列出这两条引用；随后移除探针行并落地正式纠偏。日志：route-negative-probe-after-rewrite.log。

## 3. 接口 → 路由完整挂载位置对照

四个 skill 正文引用的全部端点（均验证为真实存在且 method 一致）：

| Skill 引用 | method | 真实挂载位置 |
| --- | --- | --- |
| /activity-intervals | GET | server.rs:812，主 router 根级 `.get` |
| /activity-intervals/missing-summary | GET | server.rs:814，根级 |
| /activity-intervals/:interval_id/evidence | GET | server.rs:818，根级（handler routes/activity_ledger.rs::get_activity_interval_evidence） |
| /frames/:frame_id/text | GET | server.rs:739（741 另有 POST=重跑 OCR，skill 未引用） |
| /frames/:frame_id/context | GET | server.rs:743 |
| /search | GET | server.rs:716 |
| /knowledge/knowledge（列表） | GET | server.rs:1141 `nest("/knowledge", …)` + knowledge/routes.rs:38 |
| /knowledge/knowledge/:id | GET | 同上 nest + knowledge/routes.rs:41 |
| /knowledge/work-units | GET | 同上 nest + knowledge/routes.rs:39 |
| /knowledge/work-units/:id | GET | 同上 nest + knowledge/routes.rs:40 |
| /knowledge/status | GET | 同上 nest + knowledge/routes.rs:49 |

无虚构路径；`GET /knowledge`、`GET /knowledge/:id`、`/knowledge/activity-intervals` 在真实路由表中不存在，已被回归测试固化为负例。测试另以 `POST /answer`（server.rs:1138 根级 + knowledge/answer.rs:35 MethodRouter）做跨文件 method 解析自检。

## 4. 正文改动说明

1. **路由测试重写**（engine/tests/internal_skill_routes.rs）：静态重建有效路由集——逐文件提取字面路由及其 method（`.get/.post/...` 与 `.route(path, get(h))`，`-> MethodRouter` 构建器按函数体跨文件解析）；`.nest(prefix, expr)` 只把前缀赋给 expr 中以 `module::fn_name(`（或全 crate 唯一裸名）引用的构建器函数所在文件，杜绝 prefix×literal 笛卡尔积；剥离 `#[cfg(test)]` 模块与注释（字符串感知、保长度的词法清洗）。负例固化 8 条：GET /knowledge、GET /knowledge/:id、虚构 /knowledge/activity-intervals、/knowledge/activity-ledger、/knowledge/search，及 POST /activity-intervals、POST /knowledge/knowledge、DELETE /frames/:frame_id/text。未改生产路由、未加依赖。
2. **knowledge-fetch 分页**：改为「evidence 只有 `limit`（1–1000，默认 200），无 offset/page token；`truncated:true` 仅表示返回条数触达 limit，不证明还有更多行；触顶在限额内提高 limit，仍触顶如实报缺口，禁止构造不存在的分页参数」。依据：routes/activity_ledger.rs `ActivityIntervalEvidenceQuery`（仅 limit，clamp(1,1000)）与 `truncated = records.len() >= limit`。
3. **引用编号任务内唯一**：knowledge-fetch 新「Citation numbering (unique per task)」段——sN→interval `id`、eN 全任务唯一→`interval_id+source_type+source_id`、uN→WorkUnit `id`，编号不得分段重置且必须能映射回唯一行；其余三个 skill 同步口径。
4. **knowledge-distill JSON 对齐引擎 registry**（knowledge/registry/mod.rs::validate）：每个候选补 `"schema_version":1`（registry 硬性要求）；DecisionRule 的 boundary 按 registry 写为「恒必填非空」；ExceptionPlaybook 的 boundary 为「推荐，single_observation:true 且缺 boundary 时至少一条 handling 变为必填」，示例同步加 boundary；uN 定义为按阅读顺序分配、任务内唯一并映射 WorkUnit id。

## 5. 偏差与残留风险

1. **引擎侧 prompt/registry 不一致（生产代码，本包禁改）**：knowledge/prompts/mod.rs 的 COMPILE_USER 示例（SOP/DecisionRule/ExceptionPlaybook）均未含 `schema_version:1`，而 registry::validate 要求 `schema_version == 1`，compile.rs 也不注入——引擎自产候选会因 schema_version 被拒。skill 正文已按代码（registry）为准；建议后续单独任务修 prompt 或注入。
2. 静态路由模型的已文档化过近似：局部变量子路由（`nest("/pipes", pipe_routes)`、`/power`）无法归属，字面量留在根级；未模拟 nest 内嵌 nest（当前代码不存在）。两类近似都不会复活三条知识负例。
3. 独立复核提到的 RESERVED_SKILLS 漏名与外部 Agent 同名目录冲突：不在本包 allowed_paths，未处理，遗留协调者。
4. docs/plans/internal-skills-execution-plan.md 与 docs/archive/ 的 M/?? 为任务开始前已存在的 dirty diff，本轮未触碰、未提交。
