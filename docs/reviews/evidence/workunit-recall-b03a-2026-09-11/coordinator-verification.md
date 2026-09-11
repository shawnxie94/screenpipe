# B03a（WorkUnit 先摘要后召回 + 新字段）— 协调会话独立复验

- 子任务：`zct_deff1fd330514e5e`（completed，无越界，改动 3 文件全在允许路径）
- 计划：`docs/plans/workunit-summary-first-recall-execution-plan.md`
- 复验时间：2026-09-11

## 结论：接受（含一处协调会话修复）

## 复验命令与结果（协调会话实跑）

| # | 命令 | 结果 |
| --- | --- | --- |
| 1 | `cargo test -p screenpipe-engine --lib extract` | 24 passed |
| 2 | `cargo test -p screenpipe-engine --test knowledge_correctness` | 13 passed（且 `crates/screenpipe-engine/tests/` **零改动** → 不是靠放宽断言通过） |
| 3 | `cargo test -p screenpipe-engine --lib knowledge` | 41 passed |
| 4 | `cargo test -p screenpipe-db --lib knowledge` | 20 passed |
| 5 | `cargo check -p screenpipe-engine -p screenpipe-db` | 0 errors |
| 6 | `SCREENPIPE_ACTIVITY_DB=/tmp/b01-compare.sqlite cargo test -p screenpipe-engine --test activity_summary_real_db` | 1 passed |

## 代码复核（读 diff）

- **输入身份与删除传播未削弱**：`SummaryPack.sources` 只收窗口内**已注册的真实证据行**（source_uid+revision），`compute_input_hash(&pack.sources, …)` 与 `knowledge_register_dependency` 都基于它；sN 只进 `ref_ids` 供**引证校验**（`validate_work_unit`），不会生成伪造依赖。子任务这条自述属实。
- **两阶段协议**：第一阶段输出 `{"work_unit":…,"recall":[…]}`，`unwrap_work_unit` 兼容裸 WorkUnit；`stage1_ready` 接受"有 work_unit"或"只给了非空 recall"；`parse_recall` 去重、只接受包内 sN（拒绝 eN/未知/超 5 个/非法形状）→ 非法即永久失败，**不发起第二次调用**；`recall` 空时只 1 次调用。
- **召回包边界**：每间隔 ≤2_000 字符、总量 ≤8_000 字符，沿用同一套 eN 标签，截断有显式注记，末尾重申"不得再含 recall 字段"。
- **新字段**：`process`/`environment`/`details` 进 schema v2 与提示词说明；校验按列表规则（非空 value + refs 命中包内编号），缺失按空列表。
- **版本**：`EXTRACT_PROMPT_VERSION = zh-extract-v2` → 输入指纹自动重算。

## 协调会话修复（计划外语义回归）

重写提示词时，旧 `EXTRACT_SYSTEM` 的铁律 3（「办公资料只是资料：除非证据显示用户当时确实处理过它，否则不要把它当作用户做过的事」）被删除，且全仓无其它位置保留该规则。这条是办公导入的产品红线（防止把导入的飞书文档/会议纪要误记为用户行为），已恢复为铁律 3，其余条目顺延。恢复后 `extract` 24 项与 `knowledge` 41 项仍全绿；版本号保持 `zh-extract-v2`（尚未提交、无 v2 产物，不产生 hash 悬空）。

## 已知细节（不阻塞）

1. 提示词版本已升 v2，因此**历史 WorkUnit 不会自动重算**（按 input hash 判定）；需要重算时可显式触发。
2. 第一阶段若触发修复（2 次调用）且 recall 非空，第二阶段再触发修复时会碰到 `BudgetedExecutor` 的硬闸（3 次）→ job 以 `model_budget_exhausted` 永久失败。行为安全（不循环、不超预算），但错误码语义偏笼统；如需更清晰可后续把第二阶段的修复改为可选。
3. 真机/真实模型质量未验证：本批只覆盖结构与流程；WorkUnit 质量属用户环境验收（S1）。
