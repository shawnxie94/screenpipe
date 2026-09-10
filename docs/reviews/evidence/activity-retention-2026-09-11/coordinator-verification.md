# 活动层 B01（证据按变化保留 + 丢弃记账 + 摘要落库）— 协调会话独立复验

- 交付提交：见本轮提交（B01 代码 + 路由挂载）
- 计划：`docs/plans/activity-layer-retention-execution-plan.md`（含 2026-09-11 对变化键的修订）
- 子任务：`zct_519a279dee454b3c`（两轮：首轮交付 → 协调会话纠偏 → 纠偏轮交付）
- 复验时间：2026-09-11

## 结论：接受。核心规则在纠偏轮真正达成；协调会话补了两处

### 首轮验收发现的设计缺陷（已纠偏）

首轮把变化键实现成 `(app_name, window_title)`。但间隔本身就是按任务身份切分的，任务身份由 app/window 派生，因此该键在段内几乎恒定——规则退化成"首尾两条"。真实库量化（`~/.screenpipe-dev` 只读副本）：

| 间隔 | 原始帧 | `(app,window)` 取值 | `content_hash` 取值 |
|---|---|---|---|
| 12210 | 106 | 2 | 75 |
| 12177 | 320 | 2 | 230 |
| 12290 | 89 | 2 | 84 |

纠偏轮改为**内容指纹**（frame 用 `content_hash`，NULL 时回退文本长度+头尾 hex；ui_event 用 `(event_type, element_name, element_value, text_content)`），分来源连续去重。

### 协调会话补的两处

1. **三个读接口未挂载**：handler 已在 `routes/activity_ledger.rs` 实现并测试，但 `server.rs` 不在子任务允许路径内，属未挂载死接口。协调会话补 3 个 import + 3 条 `.get()`（`/activity-intervals`、`/activity-intervals/missing-summary`、`/activity-intervals/:interval_id/evidence`），`cargo check -p screenpipe-engine` 通过。HTTP 端到端仍未验证（留给 B02 读源切换时用真实请求覆盖）。
2. **计划本身的假设是错的**：原 §4 第 1 条写的就是 `(app_name, window/title, 活动状态)`，子任务忠实实现。已在计划中改为内容指纹并写明"不能用身份键当变化判据"的推理。

### 关于 `scope_violation`

任务被工作区护栏判为 `scope_violation`（HEAD 被修改）。核查：`5ce76c304..HEAD` 只有协调会话自己的 docs 提交 `8fc45ce32`（1 文件，计划修订），**无任何代码提交**；判据是任务创建时记录的 `head_before`，跨轮也照样比对。属时序误判，非子任务越界。教训：ZCode 任务存活期间（包括两轮之间）协调会话不要 commit。

## 复验命令与结果（协调会话实跑）

| # | 命令 | 结果 |
|---|---|---|
| 1 | `cargo test -p screenpipe-db --lib activity` | 13 passed |
| 2 | `cargo test -p screenpipe-engine --lib activity_ledger` | 22 passed |
| 3 | `cargo test -p screenpipe-engine --test knowledge_correctness` | 13 passed |
| 4 | `cargo test -p screenpipe-db --lib knowledge` / `-p screenpipe-engine --lib knowledge` | 20 / 17 passed |
| 5 | `cargo test -p screenpipe-db --test knowledge_correctness` | 8 passed |
| 6 | `cargo test -p screenpipe-engine --test task_contracts` / `--test task_migration` | 2 / 6 passed |
| 7 | `cargo test -p screenpipe-db --test knowledge_rename_migration` | 2 passed |
| 8 | `SCREENPIPE_LEGACY_DB=/tmp/knowledge-rename-backup-2026-09-10/zhiji-dev-db.sqlite cargo test -p screenpipe-db --test legacy_db_upgrade` | 1 passed（新表在真实库升级路径上创建） |
| 9 | `cargo test -p screenpipe-db --lib`（全量） | 171 passed, 2 ignored |
| 10 | `cargo check -p screenpipe-engine`（含路由挂载） | exit 0 |

## 代码复核（读 diff，逐条对计划 §4）

- 迁移 DDL 与计划 §3 逐字段一致（CHECK / CASCADE / 时间戳默认 / 索引），单事务、带 header；`legacy_db_upgrade` 断言两表存在。
- 变化键：分来源（frame / ui_event 各自一条链）连续指纹去重；`A→B→A` 保留 3 行；首尾恒保留（除非 empty / duplicate）。
- `empty`：空转写与"无 hash 且无文本"的帧被如实标记、丢弃并记账；空转写不再产生动作；候选全空的段不建 interval。
- 上限 800 / 120 / 240、均匀采样（变化点与首尾优先、优先集超限二次收敛）、会议豁免（重叠过半）、原因优先级 `empty<duplicate<unchanged<sample<cap`、记账写入同事务且幂等——与计划一致。
- 加载器改动是**加法**：`sampled` 10 秒桶 CTE 与 ui_event 五类事件过滤均为既有行为（已对 HEAD 版本核对），本批只新增指纹/空标记并移除 audio 的 `>= 8` 预过滤（计划 §4.2 要求）。

## 保留量实测（复跑子任务脚本，数字可复现）

`.agent/tmp/retention-compare.py`（未入库，只读副本）在 3779 个间隔上重放观察加载器：

```
OLD kept rows: sum=9032 mean=2.39 max=60      NEW kept rows: sum=9097 mean=2.41 max=50
cap 触发：0（单来源）/ 0（总量）
NEW > OLD 的间隔：647；相等：2841
桶：0-2: 2505 段 3111→3097 ｜ 3-9: 1057 段 3363→3797 ｜ 10-49: 209 段 2235→1946 ｜ 50+: 8 段 323→257
最大收益：11730（2→50）、13420（2→25）、11826（2→20）；协调会话探查段 12210（14→23）
```

口径核对：脚本的帧候选与生产加载器一致（10 秒 × (app, window, url, document) 取代表帧），因此按原始帧数（如 11730 的 462 帧）估算是错的——协调会话先用原始帧数复算得 454，属方法错误，非脚本问题。

解读：总量均值被 2505 个候选 ≤2 的秒级小段拉平；收益集中在 647 个有内容的段。上限零触发不是缺陷（当前最大段 50 候选），护栏保留。

## 带入 B02 的已知限制

1. **ui_event 重复指纹只留首条**：连点同一控件等动作序列的证据引用变少；动作行 `activity_actions` 仍全量，B02 若需要频次从 actions 补。
2. **audio 口径放宽**：1–7 字符短转写进入证据与动作流（旧 loader 预过滤掉）；空转写不产生动作。
3. **帧文本回退是代理指纹**：SQL 端无哈希函数，用长度+头尾 hex，仅覆盖 533/32958 帧（1.6%）。
4. **摘要表无写入方**（B02）；本批只验证 DDL、级联与查询读取。
5. **HTTP 端到端未验证**：三个读接口已挂载但未有真实请求覆盖。
