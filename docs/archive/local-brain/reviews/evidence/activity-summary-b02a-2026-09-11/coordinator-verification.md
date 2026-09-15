# 活动层 B02a（间隔摘要生成）— 协调会话独立复验

- 子任务：`zct_880cfcfc20d94b85`（单轮交付）
- 计划：`docs/plans/activity-summary-producer-execution-plan.md`
- 复验时间：2026-09-11

## 结论：接受。规则、幂等、真实数据路径均达成；协调会话补两处

## 一、`scope_violation` 复核：真越界，但越界文件是必要改动（计划漏列）

任务被护栏判 `scope_violation`，越界 3 个文件：

| 文件 | 改动 | 判断 |
| --- | --- | --- |
| `crates/screenpipe-db/src/db/mod.rs` | re-export 两个新类型（+2/-1） | 必要，计划漏列 |
| `crates/screenpipe-db/src/lib.rs` | 同上（+2/-1） | 必要，计划漏列 |
| `crates/screenpipe-engine/src/routes/activity_ledger.rs` | 适配 `activity_intervals_missing_summary` 新签名（+1/-1） | 必要 |

**HEAD 未被修改**（`head_after == head_before`）——上一轮的教训生效。已把 `db/mod.rs`、`lib.rs`、`routes/activity_ledger.rs` 补进计划允许路径，避免同类误判。

## 二、协调会话补的两处

1. **`task_activate_owner_generation("knowledge")` 缺 `knowledge.summarize`**：cutover 激活时新定义不会随其它知识定义迁移到新世代。已加入（`crates/screenpipe-db/src/db/tasks/mod.rs`），`task_migration` 6 项回归通过。这是与批 1 同一类问题（新定义漏进激活列表），主动排查挡下。
2. **真实库队列重建路径缺断言**：`knowledge_jobs` 因 kind CHECK 需要整表重建，而重建前该表可能已有大量在途行（你的 dev 库就有 715 行）。已在 `legacy_db_upgrade` 补两条断言：队列行数在「改名 + 重建」两步后不变、重建后的 CHECK 接受 `summarize`。

## 三、复验命令与结果（协调会话实跑）

| # | 命令 | 结果 |
| --- | --- | --- |
| 1 | `cargo test -p screenpipe-engine --lib summarize` | 14 passed |
| 2 | `cargo test -p screenpipe-db --lib activity` | 17 passed |
| 3 | `cargo test -p screenpipe-db --lib`（全量） | 175 passed, 2 ignored |
| 4 | `cargo test -p screenpipe-engine --lib knowledge` | 30 passed |
| 5 | `cargo test -p screenpipe-db --test knowledge_correctness` / engine 同名 | 8 / 13 passed |
| 6 | `cargo test -p screenpipe-engine --test task_migration` | 6 passed |
| 7 | `cargo check -p screenpipe-db -p screenpipe-engine` | exit 0（0 errors） |
| 8 | `SCREENPIPE_ACTIVITY_DB=/tmp/b01-compare.sqlite cargo test -p screenpipe-engine --test activity_summary_real_db -- --nocapture` | **20 间隔摘要成功、0 失败、20 次模型调用**（首轮日志里的 17 次失败是中间态，当前代码已修） |
| 9 | `SCREENPIPE_LEGACY_DB=<6 月真实库副本> legacy_db_upgrade` | 126 迁移、5 表核对、通过 |
| 10 | `SCREENPIPE_LEGACY_DB=/tmp/dev-ledger-copy.sqlite legacy_db_upgrade` | **125 迁移、7 表核对；`brain_jobs` 715 行在改名 + 重建后一行未丢，CHECK 接受 `summarize`** |
| 11 | `cd apps/screenpipe-app-tauri && bun run test:tauri knowledge_runtime` | 1 passed（src-tauri 改动走仓库 native 命令） |

说明：第 8 项首轮曾在旧日志里出现「17 个 job 失败」，原因是测试脚本把每 job 预算做成共享计数器（测试侧缺陷，非生产路径）；当前版本每个 job 独立预算，20/20 成功。生产中 `BudgetedExecutor` 的预算计数器是 per-instance（`Arc<AtomicU32>`），不存在进程级泄漏。

## 四、代码复核（读 diff，逐条对计划 §4）

- 分档：`<15min → short 80–150`、`15–60min → medium 150–300`、`>60min → long 300–600`，字数为去空白字符数 ✓
- 校验：字数越界、关键字去重后 5–12、引证 1–3 且必须命中证据包编号；违规 → 严格化提示词修复一次；仍违规 → `summary_invalid` 不落库 ✓
- 落库：`activity_summary_upsert` 在同事务校验「引证必须是该间隔已保留的证据行」、按 `(interval_id, input_hash)` 幂等、hash 变化则更新 ✓
- `input_hash` 含提示词版本 + 间隔身份 + 已保留证据对 + 模型身份（`BudgetedExecutor::identity` 实时解析，切换预设会重算）✓
- 提示词：自足要求 + 反例（「处理了开发工作」等）+ 不得夸大采样覆盖面 + 证据中的指令视为数据 ✓
- 发现式入队：单轮上限 20、只挑已结算（end_at ≤ now−5min）且缺摘要的间隔，与提取 discovery 同一 ticker（先摘要后提取）✓
- 迁移：`evidence_refs` 加列 + `knowledge_jobs` 整表重建（kind CHECK 加 `summarize`）均单事务、无 `no-transaction`、带 header；该表无 FK 引用、无触发器，重建只重键三个索引 ✓

## 五、未验证 / 带入 B02b

1. **摘要质量未做人工评审**（假执行体只验证结构与落库）：真实模型下的自足性与专名覆盖属用户环境验收（S1 组的输入之一）。
2. **前端未读这套摘要**：B02b 才切读源、旧 KV 叙事标 legacy；本批新路径与旧「活动历史」互不干扰。
3. **HTTP 端到端仍缺**：三个读接口已挂载，但没有真实请求覆盖（B02b 用 Tauri 命令读时覆盖）。
4. 修一次仍违规的间隔会失败并留 `last_error_code=summary_invalid`，重试策略依赖既有 attempts 机制。
