# 活动层收口 B03a：WorkUnit 先摘要后召回 + 流程/环境/细节字段 执行计划

```yaml
status: approved
owner: shawn
created: 2026-09-11
framework: docs/trd/personal-workbench-integration-framework.md#d-15
covers:
  - D-15 WorkUnit 先读摘要、按需召回；新增流程/环境/细节字段；知识只从 WorkUnit
doc-covers: crates/screenpipe-engine/src/knowledge
doc-verified: 5f5aeae6e
```

## 1. 目标与非目标

### 1.1 目标（B03a）

把 WorkUnit 提取的输入从"原始证据行"换成"**间隔摘要优先 + 按需回捞原始证据**"，并给 WorkUnit 增加 `process` / `environment` / `details` 三组字段。

### 1.2 非目标

| 项 | 批次 |
| --- | --- |
| 会话计数按活动（间隔）去重、`session_count` 校验改为按间隔推导 | B03b |
| 四个 skill（共享取数 + 活动总结 / 工作单元 / 知识提炼） | B04 |
| 物理删除旧叙事生成器（门禁：用户 S1 验证后） | B02b-2b |

## 2. 现状锚点

| 事实 | 位置 |
| --- | --- |
| 提取输入：原始证据行（`activity_evidence` join 抓取表），`MAX_SOURCES=32`、`MAX_PACK_CHARS=24_000` | `crates/screenpipe-engine/src/knowledge/extract.rs:180-260` |
| 一次模型调用 + 一次修复重试 | 同文件 `run_extract`（第 115 行附近）、`repair_prompt` |
| 提示词（编译期常量，版本参与 input hash） | `knowledge/prompts/mod.rs`（`EXTRACT_SYSTEM`/`EXTRACT_USER`）、`EXTRACT_PROMPT_VERSION = "zh-extract-v1"`（`knowledge/sources.rs:22`） |
| 摘要来源（本批的输入） | `activity_intervals_between`（含 `summary`/`keywords`）与 `activity_evidence_for_interval`（B01/B02a） |
| 现有 WorkUnit JSON 结构 | `EXTRACT_USER`：task/inputs/actions/decisions/exceptions/outputs/result/confidence/notes，列表项均带 `evidence_refs` |
| 知识只从 WorkUnit | `knowledge/compile.rs` 与 `registry/mod.rs`（`work_unit_refs`） |

## 3. 改动

### 3.1 输入（先摘要）

`extract.rs` 新增 `build_summary_pack`（保留现有 `build_evidence_pack` 作回落）：

- 取窗口内**有摘要**的间隔，每个间隔一个 `sN`：`{时间范围, 应用/标题, 摘要, 关键字, 保留/丢弃记账, 引证 eN 列表}`；引证只列该间隔已保留证据（≤3 条 / 间隔），带 `source_type/source_id/occurred_at`。
- 有界：间隔数 ≤ 40、摘要文本走 `bounded_excerpt`、整包 ≤ 24_000 字符；超限按时间均匀取样并注明。
- **窗口内没有摘要的间隔**：退化为现有原始证据包（同一提示词里以 `[原始证据]` 段呈现），保证上线过渡期提取不中断。

### 3.2 两阶段调用（按需召回）

`run_extract` 改为：

1. **第一阶段**：给摘要包，要求输出 `{"work_unit": {…}, "recall": ["s3","s7"]}`（`recall` 0–5 个 `sN`，只允许引用包内编号）。
2. **第二阶段**（仅当 `recall` 非空）：用被点名的间隔的原始证据文本（每间隔 ≤ 2_000 字符、总 ≤ 8_000）作为 `{recall_pack}` 追加调用，要求输出最终 WorkUnit JSON（不得再出现 `recall`）。
3. 修复重试沿用现有逻辑；总调用数 ≤ 3（与每人任务预算一致）。
4. `recall` 为空时只调 1 次（典型短间隔）。

### 3.3 新字段（WorkUnit schema v2）

在 JSON 里新增三组列表（结构同 `inputs`/`actions`：`[{"value":"…","evidence_refs":["s1"]}]`）：

- `process`：流程——这次是怎么推进的（有序做法、手段、顺序）；
- `environment`：环境——涉及的工具/设备/项目/协作对象/上下文；
- `details`：细节——具体文件、命令、链接、参数、版本号等可复核的实体。

`schema_version` 由 1 升到 **2**，`EXTRACT_PROMPT_VERSION` 升为 `zh-extract-v2`（参与 input hash → 自动重算）。

### 3.4 校验与兼容

- `registry`/提取侧的 WorkUnit 校验：新字段按既有列表规则校验（非空 `value`、`evidence_refs` 必须命中包内编号）；**新增字段缺失时按空列表处理**（向后兼容历史 WorkUnit）。
- `routes.rs`/`compile.rs` 里构造或读取 WorkUnit 的地方：新字段为可选，不得因缺失报错。
- 知识仍只从 WorkUnit 产出（`compile.rs` 不改输入语义）。

## 4. 测试与验收命令

| # | 命令 | 期望 |
| --- | --- | --- |
| 1 | `cargo test -p screenpipe-engine --lib extract` | 新增：摘要包构造（含超限取样、无摘要回落）、两阶段召回（脚本执行体：第一阶段给 recall → 第二阶段给最终 JSON）、recall 为空只 1 次调用、非法 recall 编号被拒、新字段校验、schema v2 |
| 2 | `cargo test -p screenpipe-engine --test knowledge_correctness` | 13 项**继续通过**；若断言旧 schema 形状，按 v2 更新断言（不允许放宽实质校验） |
| 3 | `cargo test -p screenpipe-db --lib knowledge` | 20 passed（输入层未改 db） |
| 4 | `cargo test -p screenpipe-engine --lib knowledge` | 全绿 |
| 5 | `cargo check -p screenpipe-engine -p screenpipe-db` | exit 0 |
| 6 | `SCREENPIPE_ACTIVITY_DB=/tmp/b01-compare.sqlite cargo test -p screenpipe-engine --test activity_summary_real_db` | 仍通过（摘要生产者回归） |

## 5. 风险与回退

| 风险 | 处置 |
| --- | --- |
| 摘要质量不足导致 WorkUnit 变差 | 两阶段召回让模型能回捞原始证据；过渡期无摘要的间隔走原路径 |
| 调用次数上升（2 次成常态）→ 成本 | 只有 `recall` 非空才二次调用；包内有界；每人任务预算 ≤3 仍是硬闸 |
| schema v2 影响下游读取 | 新字段可选 + 缺失按空列表；`knowledge_correctness` 全量回归 |
| 提示词改动导致历史 WorkUnit 与新版并存 | 版本串进 input hash → 旧数据不失效，按需重算 |

**回退**：`EXTRACT_PROMPT_VERSION` 回退 + 关闭 recall 分支即恢复单阶段；旧 schema WorkUnit 仍可读。

## 6. 交付边界

- 允许路径：`crates/screenpipe-engine/src/knowledge/{extract.rs,prompts/mod.rs,sources.rs,registry/mod.rs,routes.rs,compile.rs,types.rs}`、`crates/screenpipe-engine/tests/`。
- 禁止：改 `crates/screenpipe-db/**`、前端、`docs/**`、已存在的迁移；不做 B03b 的会话计数改造；不动 `activity_ledger.rs`（活动层数据面已定稿）。
- 不 commit / 不 push；模型测试只用脚本执行体（禁止真调远程模型）；真实库只读。

## 完成记录（B03a）

- 子任务：`zct_deff1fd330514e5e`（completed，无越界）
- 交付：`extract.rs` 摘要包 + 两阶段召回 + schema v2 校验；`prompts/mod.rs` 两阶段协议与三组新字段；`sources.rs` 版本升 `zh-extract-v2`
- 协调会话修复：恢复被重写时丢掉的办公资料红线（铁律 3）
- 证据：`docs/reviews/evidence/workunit-recall-b03a-2026-09-11/`
- 验收：协调会话实跑 6 条命令全绿；`crates/screenpipe-engine/tests/` 零改动（13 项非放宽通过）
- 带入 B03b：会话计数按活动去重；另记两条细节（历史 WorkUnit 不自动重算、第二阶段修复会撞预算硬闸）
