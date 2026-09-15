# B03a 交付报告：WorkUnit 先摘要后召回 + 流程/环境/细节字段（schema v2）

日期：2026-09-11｜分支：zh-local｜计划：docs/plans/workunit-summary-first-recall-execution-plan.md（status: approved）

## 1. 实际读取/修改的文件

### 修改（3 个，全部在允许路径内）

| 文件 | 改动 |
| --- | --- |
| `crates/screenpipe-engine/src/knowledge/sources.rs` | `EXTRACT_PROMPT_VERSION`：`zh-extract-v1` → `zh-extract-v2`（参与 input hash，旧数据不失效、按需重算）。`EXTRACTOR_SCHEMA_VERSION` 保持 `work_unit.v1` 不变（计划 §3.3 只指定这两个版本变化；见 §5 备注） |
| `crates/screenpipe-engine/src/knowledge/prompts/mod.rs` | `EXTRACT_SYSTEM`/`EXTRACT_USER` 重写为两阶段协议：第一阶段输出 `{"work_unit":…,"recall":["sN",…]}`（0–5 个、仅包内 sN），第二阶段填 `{recall_pack}` 后输出最终 JSON 且不得再含 recall；WorkUnit 结构示例加入 `process`/`environment`/`details` 三组列表，`schema_version` 示例升为 2 |
| `crates/screenpipe-engine/src/knowledge/extract.rs` | 核心改动，见下 |

`extract.rs` 内部：

- 新增 `build_summary_pack`：取窗口内**有摘要**的间隔，每个输出一个 `sN` 块（时间范围、应用/标题、摘要（`bounded_excerpt` ≤600 字符）、关键字、保留/丢弃记账、≤3 条引证 `eN`，引证行带 `source_type #id @ occurred_at`）。有界：间隔 ≤40、整包 ≤24_000 字符（留 256 字符票据余量）；超限按时间均匀取样（`sample_uniform`，保留两端点）并在包首注明。**窗口内无摘要的间隔**的原始证据以 `[原始证据]` 段落呈现在同一提示词中；整窗无摘要时整体退化为原 `build_evidence_pack` 路径（函数保留，语义逐字未动）。
- 关键设计决定：`load_and_register_window_rows`（从原 `build_evidence_pack` 抽出）仍把窗口内**全部**保留证据注册为稳定源，`input_hash` 与 `knowledge_register_dependency` 依赖都建立在原始源上——即使提示词只给摘要。原因：删除传播走 `knowledge_invalidate_consumers`（source → work_unit 依赖边），摘要不注册为源；同时「同间隔新证据 → 新 revision → 重算」的语义保持不变。`input_deleted`（原始行被删 → permanent 拒绝）语义原样保留。
- `run_extract` 拆出可测试引擎 `extract_interval`（照 `summarize_interval` 模式），两阶段：第一阶段（含一次 repair）→ `parse_recall`（0–5、仅包内 sN、去重；非法形状/未知编号/超 cap 一律 permanent `invalid_recall`，**不做第二次调用**）→ recall 非空才追加第二阶段调用（含一次 repair）；`recall` 为空只调 1 次。总调用数 ≤3 由既有 `BudgetedExecutor`（`MAX_MODEL_CALLS=3`）硬闸保证。第二阶段输出容忍 `{"work_unit":…}` 包裹或裸 WorkUnit（`unwrap_work_unit`），最终提交体不得含 recall。
- `build_recall_pack`：被点名间隔的原始证据文本，每间隔 ≤2000 字符、总 ≤8000，行沿用包内 `eN` 标签（全部注册行都有 eN，摘要块只展示 ≤3 条），超限注明「已截断」，尾部附「输出最终 WorkUnit JSON、不得再含 recall」指令。
- `validate_work_unit`：`schema_version` 必须为 2；`process`/`environment`/`details` 按列表规则校验（非空 `value` + `evidence_refs` 非空且命中包内编号），**缺失按空列表处理**（历史 WorkUnit 可读）；v1 五组列表校验规则逐字未动。
- 每条注册行都有 eN 标签进入 `ref_ids`（`sN` → 间隔 id），故 WorkUnit 的 `evidence_refs` 可合法引用 sN（摘要级）或 eN（原始证据级），与计划 §3.3 示例一致。

### 读取（未改）

`docs/plans/workunit-summary-first-recall-execution-plan.md`、`knowledge/{summarize.rs,compile.rs,routes.rs,registry/mod.rs,worker.rs,executor.rs,mod.rs,types.rs,deletion.rs（grep 级）}`、`crates/screenpipe-db/src/db/activity_ledger.rs`（`activity_intervals_between`/`activity_evidence_for_interval`/`activity_summary_upsert`/`ActivityIntervalRecord`，只读）。

未改但按计划 §3.4 验证过兼容性：`routes.rs` 对 WorkUnit body 是 JSON 透传（`from_str::<Value>.unwrap_or`），`compile.rs` 整体序列化 body 进提示词、只对 knowledge 体做 registry 校验——两侧都不按字段强读 WorkUnit，新字段可选、缺失不报错，无需改动。`registry/mod.rs` 校验的是 knowledge 体（uN 引用），与 WorkUnit v2 无交集，未改。

## 2. 执行的命令及结果（计划 §4 全部 6 项）

| # | 命令 | 结果 |
| --- | --- | --- |
| 1 | `cargo test -p screenpipe-engine --lib extract` | **24 passed; 0 failed**（含新增：摘要包构造/引证、无摘要回落、45 间隔超限均匀取样+整包 ≤24_000 断言、两阶段召回（2 次调用、第二阶段带 `[sN] 的原始证据` 与 eN 标签、提交体无 recall、依赖注册在原始源）、空 recall 只 1 次调用、非法 recall（`s9`/`e1`）permanent 拒绝且只 1 次调用、recall 包每间隔/总量预算、`parse_recall` 拒绝矩阵、`sample_uniform` 端点、新字段校验、schema v2 必为 2） |
| 2 | `cargo test -p screenpipe-engine --test knowledge_correctness` | **13 passed; 0 failed**（未改断言：这些测试经 DB 层直写 WorkUnit、registry 校验的是 knowledge 体，不受 v2 影响） |
| 3 | `cargo test -p screenpipe-engine --lib knowledge` | **41 passed; 0 failed** |
| 4 | `cargo test -p screenpipe-db --lib knowledge` | **20 passed; 0 failed**（db 层未改） |
| 5 | `cargo check -p screenpipe-engine -p screenpipe-db` | **exit 0**（仅既有警告；extract.rs 无新警告，`interval_scope` 的 `task_key` 未用参数为原有代码） |
| 6 | `SCREENPIPE_ACTIVITY_DB=/tmp/b01-compare.sqlite cargo test -p screenpipe-engine --test activity_summary_real_db` | **1 passed**（真实运行：`--nocapture` 确认 3779 个缺摘要间隔、发现 20 个任务、脚本执行体 20 次调用全成功；真实库只读，测试在私有拷贝上工作） |

模型相关测试全部使用脚本执行体（`ScriptedExecutor`，`--lib` 内），未真调远程模型。

## 3. 结论

B03a 计划 §3 的四组改动（3.1 摘要优先输入、3.2 两阶段按需召回、3.3 schema v2 新字段与版本、3.4 校验与兼容）全部落地，§4 六项验收全绿。回落路径完整：`build_evidence_pack` 原样保留，整窗无摘要自动退化；关闭 recall 分支（把 `parse_recall` 恒返空）即恢复单阶段。知识仍只从 WorkUnit 产出（compile 输入语义未动）。

## 4. 未做/未验证

- **未做 B03b**：会话计数按间隔去重、`session_count` 校验改造（计划非目标）。
- **`EXTRACTOR_SCHEMA_VERSION` 未升版**（保持 `work_unit.v1`）：计划 §3.3 只指定 JSON `schema_version` 1→2 与 `zh-extract-v2`；prompt 版本变化已驱动 input hash 重算。该常量自身注释说「WorkUnit 契约变更时应升版」，如协调会话认为应升 `work_unit.v2`，是一行改动（会再变一次 hash 家族，行为无害）。
- **真模型验收未做**：按边界禁止真调远程模型；两阶段协议在真实模型下的召回命中率、`recall` 输出合规率需后续真实数据观察。
- **性能未测量**：摘要路径每任务仍加载/注册窗口内全部证据文本（与旧路径相同的每行 `register_capture_source`），成本与旧路径同级，未单独计时。
- 既有失败面未扩大：全仓测试未跑（workspace ~49 万行，按 AGENTS.md 只跑范围测试）。

## 5. 风险

1. **取样注记的字节口径**：整包 ≤24_000 按 UTF-8 字节数（与旧 `MAX_PACK_CHARS` 口径一致），中文约 3 字节/字，等效约 8k 汉字；票据余量 256 字节保证硬上限不破，但极端块大小方差下均匀取样后可能接近上限（测试内均匀块下余量充足）。
2. **第二阶段依赖模型自觉**：`recall` 非法即永久拒绝（按计划「非法编号拒绝」），若真实模型频繁编造编号会抬升任务失败率；失败为 permanent，不消耗重试队列，可通过任务控制接口观察 `invalid_recall` 计数再调优提示词。
3. **摘要块引证可能少于 3 条**：引证映射依赖该证据行已注册（窗口证据行 >32 条时 `MAX_SOURCES` 截断之外的行无 eN），此时 sN 块省略引证行；摘要本身仍完整展示，不影响提取，但该间隔可引用的原始证据变少。
4. **schema v2 与历史 v1 WorkUnit 并存**：新字段缺失按空列表处理、读取侧不按字段强读，`knowledge_correctness` 13 项全过；compile 提示词会把新字段一并带给模型（整体序列化），属于预期行为。
