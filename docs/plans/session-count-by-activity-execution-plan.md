# 活动层收口 B03b：会话计数按活动去重 执行计划

```yaml
status: approved
owner: shawn
created: 2026-09-11
framework: docs/trd/personal-workbench-integration-framework.md#d-13
covers:
  - D-13 会话计数按活动去重（不按引用条数）
doc-covers: crates/screenpipe-engine/src/knowledge/registry/mod.rs, crates/screenpipe-engine/src/knowledge/compile.rs
doc-verified: ff920587c
```

## 1. 目标

SOP 的 `session_count` 必须等于**互不相同的活动（时间段）数**，而不是引用的 WorkUnit 条数：同一活动因重算/多版本产生多个 WorkUnit 时，只能算一个会话。

## 2. 现状锚点

| 事实 | 位置 |
| --- | --- |
| `RegistryContext` 只有 `work_unit_refs`（uN → work_unit id） | `knowledge/registry/mod.rs:31-34` |
| `validate_sop` 要求 `session_count == evidence_refs.len()`（去重后的引用条数） | 同文件 `validate_sop`（`sessions != evidence_refs.len()` 分支） |
| 编译侧：取 scope 下 ≤20 个 WorkUnit，构造 `ref_map` 与提示词行（含 `interval_start`） | `knowledge/compile.rs:32-78` |
| 提示词规则 1（SOP 至少来自 3 个相互独立的工作会话） | `knowledge/prompts/mod.rs` `COMPILE_SYSTEM` |

## 3. 改动

1. **上下文扩展**：`RegistryContext` 增 `work_unit_activities: &'a HashMap<String, String>`（uN → 活动键）。
   - 活动键 = `interval_start|interval_end`（有界则用）；缺失边界的 WorkUnit 退化为 `wu:<work_unit_id>`（历史数据各自算一个会话，行为不劣化）。
2. **校验改为按活动去重**：`validate_sop` 收集 steps/exceptions 的引用（沿用 `BTreeSet` 语义去重引用），再映射为活动键去重，然后：
   - 活动数 < 3 → 报「SOP 必须来自至少 3 个互不相同的活动（时间段）」；
   - `session_count != 活动数` → 报「session_count 必须等于互不相同的活动数，不能由模型自行声明」。
3. **编译侧填充**：`compile.rs` 构造 `work_unit_activities`（与 `ref_map` 同一循环），并把活动键的构造口径写成注释。
4. **提示词**：`COMPILE_SYSTEM` 规则 1 明确「会话数按互不相同的活动（时间段）计，不按引用条数计」。

## 4. 测试与验收命令

| # | 命令 | 期望 |
| --- | --- | --- |
| 1 | `cargo test -p screenpipe-engine --lib registry` | 新增：同一活动两个 WorkUnit 引用 → 计 1；三个不同活动 → 通过；`session_count` 与活动数不一致 → 拒绝；缺边界 WorkUnit 退化计法 |
| 2 | `cargo test -p screenpipe-engine --lib compile` | 全绿；新增：活动键填充与去重口径 |
| 3 | `cargo test -p screenpipe-engine --lib knowledge` | 全绿 |
| 4 | `cargo test -p screenpipe-engine --test knowledge_correctness` | 13 passed（不得放宽） |
| 5 | `cargo check -p screenpipe-engine` | exit 0 |

## 5. 风险与回退

| 风险 | 处置 |
| --- | --- |
| 历史 SOP 的 session_count 与新口径不一致 | 本批只改**新产出**的校验；已发布知识不回改（下次重算自然收敛） |
| 活动键口径与活动层不一致 | 用间隔边界（`interval_start|interval_end`）作为活动键，与活动层的间隔一一对应 |
| 缺边界的 WorkUnit（历史） | 退化为按 work_unit id 计数，不因缺字段误判 |

**回退**：单文件级回退（校验函数与上下文各一处）。

## 6. 交付边界

- 允许路径：`crates/screenpipe-engine/src/knowledge/{registry/mod.rs,compile.rs,prompts/mod.rs,routes.rs}`、`crates/screenpipe-engine/tests/`、对应 `#[cfg(test)]` 单元测试。（`tests/` 与 `routes.rs` 是 `RegistryContext` 加字段后的必要编译修复点，首版计划漏列，已修正。）
- 禁止：改 db 层、前端、`docs/**`；不改 `extract.rs` 的输入构造；不做 B04 的 skill。
- 不 commit / 不 push；不得真调远程模型。

## 完成记录（B03b）

- 子任务：`zct_74529bbd8c1e443d`（判 scope_violation，实为计划漏列 `tests/`）
- 交付：`registry/mod.rs` 活动键去重校验、`compile.rs::activity_key`、`prompts/mod.rs` 规则 1 口径、`routes.rs` 与 `tests/knowledge_correctness.rs` 机械补齐上下文
- 证据：`docs/reviews/evidence/session-count-b03b-2026-09-11/`
- 验收：协调会话实跑 5 条命令全绿；测试文件 diff +7/−1 且无断言改动
- 活动层至此全部收口（B01 / B02a / B02b-1 / B02b-2a / B03a / B03b）
