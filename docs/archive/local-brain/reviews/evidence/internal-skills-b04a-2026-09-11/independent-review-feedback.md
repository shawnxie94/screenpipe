# B04a 独立复核与纠偏交接

日期：2026-09-11
原交付：ZCode zct_5c239f0691974123；base 0c2ccc796
结论：changes requested；未提交，B04b 未启动。

## 委派选择与实际阻塞

用户最新要求切回 ZCode。当前会话工具只有 Pi `subagent`，没有 `zcode_dispatch` 或 ZCode native Agent 工具，不能实际派发 ZCode。不得 shell 替代或继续 Pi 委派。上轮“Pi subagent 不存在”的说法不适用于当前工具列表。
最近明确的模型选择为 openai-codex/gpt-5.6-luna，medium。用户本轮只改运行时，不擅自恢复旧 GLM 配置；恢复 ZCode 入口后确认其能否表达此选择，不能则询问用户。

## 当前工作区与来源

ZCode 原交付：4 个 assets/skills/{knowledge-fetch,activity-summary,work-unit,knowledge-distill}/SKILL.md；core/src/agents/pi.rs；engine/src/cli/agent.rs。
上轮协调者修改：knowledge-fetch 两处知识路由、knowledge-distill 一处知识路由、pi.rs 两处对应白名单；另新增 engine/tests/internal_skill_routes.rs（原计划未列入，且实现不合格）。均未提交。
本轮只做验证并生成 .agent/tmp/b04a-independent-review/ 证据，未改产品源码、HEAD 或真实数据库；不处理不相关 docs/archive/。

## 实跑结果

完整 stdout/stderr：本目录各 .log；命令与退出码：results.txt。

- cargo test -p screenpipe-engine --test internal_skill_routes：exit 0，1 passed。
- cargo test -p screenpipe-core --lib skills：exit 0，4 passed。
- cargo test -p screenpipe-core --lib pi：exit 0，405 passed。
- cargo test -p screenpipe-engine --lib agent：exit 0，46 passed。
- cargo test -p screenpipe-engine --lib knowledge：exit 0，46 passed。
- cargo check -p screenpipe-core -p screenpipe-engine：exit 0。

## 必须纠偏

1. **路由测试误报通过**：engine/tests/internal_skill_routes.rs 的 effective_routes 保留所有子 router 未挂载字面路径，并任意组合所有 prefix × literal；也不校验 HTTP method。它仍接受原始错误 GET /knowledge、GET /knowledge/:id，以及虚构 /knowledge/activity-intervals。复制原测试到本目录的 route-negative-probe.rs，追加 3 个“必须拒绝”断言，rustc --edition=2021 --test 编译后执行；三项均 FAILED，exit 101。见 route-negative-probe.log。需要按实际 nest 所属关系及方法校验，或在引擎侧复用真实 Router 做不依赖真实库的匹配测试；不能再用全局笛卡尔积。必须保留这三个负例及 method 负例。此缺陷由协调者上轮新增测试引入，不归咎 ZCode 原交付。
2. **证据分页描述虚构**：knowledge-fetch Caps 写 truncated:true 即更多行存在并建议 page；routes/activity_ledger.rs::ActivityIntervalEvidenceQuery 仅 limit，无 offset/cursor，handler 只是 records.len() >= limit，因此 truncated 甚至不能保证还有一行。应如实写可能触顶、可在限额内提高 limit，最大 1000，仍触顶则标注缺口，不得教执行体构造不存在的分页参数。
3. **引用编号歧义**：knowledge-fetch Evidence citation format 要求 within one interval 编 e1..eM，又让跨间隔 WorkUnit 使用裸 eN。必须定义任务内唯一编号及映射（interval_id/source_type/source_id），不能各段重用 e1 导致无法追溯；同步各 skill 的引用表述与测试。

## 其他需核对但未作为本轮确定缺陷

- knowledge-distill 对 ExceptionPlaybook 文字要求 boundary，但 JSON 示例未给 boundary，核对引擎真实 schema 后统一。
- RESERVED_SKILLS 漏新名字、外部 Agent 同名用户目录冲突风险见原交付报告；不得在未调整允许范围时越界修改。
- 原计划无 orchestration_mode/execution_target/execution_backend，交接新 worker 前按新 runtime 补齐 canonical plan；engine/tests/internal_skill_routes.rs 需要显式纳入测试允许范围。协调者不得重演未经声明的功能修复。

## 下一安全动作

恢复当前会话 ZCode 入口后，在其可表达用户模型的前提下发一个有界纠偏任务：承接现有 dirty diff，不重做 B04a、不 commit/push、不真调产品模型、不写用户 store。完成后协调者独立复核，才提交 B04a 并进入 B04b。整体任务仍受真实模型/账号/真机/5 工作日观察等外部门控约束，不能完成闭环声明。
