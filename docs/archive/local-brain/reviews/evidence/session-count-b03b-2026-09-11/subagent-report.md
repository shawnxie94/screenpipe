# B03b 交付报告：SOP session_count 按互不相同活动（时间段）去重

日期：2026-09-11 ｜ 分支：zh-local ｜ 计划：docs/plans/session-count-by-activity-execution-plan.md

## 1. 实际读取/修改的文件

读取：计划全文、`knowledge/registry/mod.rs`、`knowledge/compile.rs`、`knowledge/prompts/mod.rs`、`knowledge/routes.rs`（构造点）、`tests/knowledge_correctness.rs`（构造点）、`screenpipe-db/src/db/knowledge/work_units.rs`（确认 `interval_start/end` 为 `Option<String>`）。

修改（均在 `crates/screenpipe-engine/`）：

| 文件 | 改动 |
| --- | --- |
| `src/knowledge/registry/mod.rs` | `RegistryContext` 增 `work_unit_activities: &HashMap<String,String>`；`validate_sop` 移除旧的 `sessions<3` 前置检查与 `session_count == 引用条数` 校验，改为：收集 steps/exceptions 引用（BTreeSet）→ 映射活动键去重 → 活动数 <3 报「SOP 必须来自至少 3 个互不相同的活动（时间段）」→ `session_count != 活动数` 报「session_count 必须等于互不相同的活动数，不能由模型自行声明」。查不到活动键时退化为 `wu:<work_unit_id>`（防漏填误判成同一活动）。单测：`ctx()` 补活动键；新增同活动去重+三活动通过、计数不一致拒绝、不足三活动拒绝、缺边界退化计法共 3 个新测试（同活动去重测试同时覆盖三活动通过断言） |
| `src/knowledge/compile.rs` | 新增 `activity_key()` 助手并写明口径注释（`interval_start|interval_end`，缺任一边界或空串退化为 `wu:<id>`）；`run_compile` 在构造 `ref_map` 的同一循环填充 `work_unit_activities`；`reg_ctx` 传入新字段。新增 `#[cfg(test)]`：有界/缺边界/空串三类键 |
| `src/knowledge/prompts/mod.rs` | `COMPILE_SYSTEM` 规则 1 改为「必须至少来自 3 个互不相同的活动（时间段）……会话数按互不相同的活动（时间段）计，不按引用条数计——同一时间段重算产生的多个 WorkUnit 只算一个会话」 |

**计划外必要改动（机械补齐，非逻辑变更）**：`RegistryContext` 加字段会使以下字面量构造点编译失败，而验收 1–5 要求 lib 与集成测试编译通过，无法回避：

- `src/knowledge/routes.rs`（2 处，`create_candidate_version`/`edit_version`）：审阅链路只有 work unit id、拿不到活动间隔，按缺边界口径填 `wu:<id>`——SOP 编辑校验行为与旧引用计数完全一致，存量 SOP 不会因新口径在审阅 UI 里突然校验失败。
- `tests/knowledge_correctness.rs`（2 处，decision_rule fixture 与 p10）：同口径补齐；p10 断言未放宽，仍要求「1 个会话 + 模型声明 3」被拒。

## 2. 执行的命令及结果

| 命令 | 结果 |
| --- | --- |
| `cargo check -p screenpipe-engine` | Finished dev profile，exit 0（13 个 warning 均为存量 unused imports，在 db 层/routes.rs/answer.rs，与本次改动无关） |
| `cargo test -p screenpipe-engine --lib registry` | 10 passed; 0 failed（含 3 个新增测试：`sop_session_count_dedupes_work_units_of_same_activity`、`sop_rejects_fewer_than_three_distinct_activities`、`sop_missing_boundary_units_each_count_as_one_session`） |
| `cargo test -p screenpipe-engine --lib compile` | 2 passed; 0 failed（含新增 `activity_key_binds_to_interval_and_degrades_without_bounds`） |
| `cargo test -p screenpipe-engine --lib knowledge` | 45 passed; 0 failed |
| `cargo test -p screenpipe-engine --test knowledge_correctness` | **13 passed**; 0 failed（含 `extraction_p10_sop_session_count_must_be_evidence_derived`，断言未放宽） |

另做只读检查：`crates/`、`apps/` 中已无旧文案（「相互独立的工作会话」「由相互独立的证据引用推导」）残留。

## 3. 结论与风险

**结论**：B03b 完成。新产出的 SOP 候选其 `session_count` 被强制等于互不相同活动（时间段）数；同一间隔重算产生的多个 WorkUnit 只计一个会话；缺边界的历史 WorkUnit 退化为按 unit id 各计一个。5 条验收命令全部通过。

**风险**：

1. **计划外触碰了 `routes.rs` 与集成测试**（见上），均为字段机械补齐 + 退化键，无逻辑变更；若协调会话认为越界，可改为在审阅链路查 DB 取间隔（超出本批范围，未做）。
2. **存量 SOP**：已发布知识的 `session_count` 按旧口径，未回改（计划 §5 预期行为）；审阅链路按退化键校验，编辑存量 SOP 不会被新口径误拒。
3. **前端展示**：审阅 UI 若显示「N 个会话」，语义现为「N 个互不相同活动」，未改前端（计划禁止）；文案如需跟进属 B04 之后的工作。
4. **活动键口径**依赖 `interval_start/end` 与活动层间隔一一对应（RFC3339 字符串直接拼接）；若未来活动层改变间隔表示，需同步 `activity_key()`。
