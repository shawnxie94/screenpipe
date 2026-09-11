# B03b（会话计数按活动去重）— 协调会话独立复验

- 子任务：`zct_74529bbd8c1e443d`
- 计划：`docs/plans/session-count-by-activity-execution-plan.md`
- 复验时间：2026-09-11

## 结论：接受。判 scope_violation 是我的计划漏列路径，非子任务越界

## 一、越界文件核查（`crates/screenpipe-engine/tests/knowledge_correctness.rs`）

`RegistryContext` 加字段后，该测试文件里两处**字面量构造**编译失败；而计划的验收 1–5 要求编译通过，无法回避。实际 diff 只有 +7/−1，且是纯机械补齐：

- `extraction_p10_...` 与 `indexed_knowledge` fixture 各加一行 `work_unit_activities`；
- **没有任何断言的增删改**：p10 仍断言 `.is_err()`，而且理由变得更硬——1 个互不相同活动 < 3（旧口径下是"引用条数 1 ≠ session_count"）。

结论：属必要编译修复，行为无变化。已把 `crates/screenpipe-engine/tests/` 补进计划允许路径，避免同类误判（这是第二次因我的路径清单过窄导致 scope_violation）。

## 二、复验命令与结果（协调会话实跑）

| # | 命令 | 结果 |
| --- | --- | --- |
| 1 | `cargo test -p screenpipe-engine --lib registry` | 10 passed（含 3 个新增） |
| 2 | `cargo test -p screenpipe-engine --lib compile` | 2 passed（含 `activity_key`） |
| 3 | `cargo test -p screenpipe-engine --lib knowledge` | 45 passed |
| 4 | `cargo test -p screenpipe-engine --test knowledge_correctness` | 13 passed |
| 5 | `cargo check -p screenpipe-engine` | 0 errors |

## 三、代码复核

- `validate_sop`：引用先用 `BTreeSet` 去重，再映射为活动键去重；`活动数 < 3` 与 `session_count != 活动数` 分别报错，文案说明"按互不相同的活动（时间段）计"。缺键时防御性退化为 `wu:<work_unit_id>`（不会把漏填误判成同一活动）。
- `compile.rs::activity_key`：`interval_start|interval_end`，缺任一边界或空串退化为 `wu:<id>`，口径写成注释；与 `ref_map` 在同一循环填充。
- `COMPILE_SYSTEM` 规则 1 明确新口径（同一时间段重算的多个 WorkUnit 只算一个会话）。
- `routes.rs` 两处审阅校验按缺边界口径补齐，审阅链路行为与旧引用计数一致（存量 SOP 编辑不会被新口径误拒）。

## 四、已知限制（记录，不阻塞）

1. 存量 SOP 的 `session_count` 不回改（计划预期）；下次重算自然收敛。
2. 活动键依赖间隔边界的 RFC3339 字符串表示；活动层若改表示需同步 `activity_key()`。
3. 前端"N 个会话"文案的语义已变为"N 个互不相同活动"，文案跟进不在本批（属 B04 之后的界面整理）。
