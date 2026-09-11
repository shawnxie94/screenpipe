# B04b 交付报告 — skill 哈希进输入指纹 + 查询轨迹与强制上限

任务：`b04b-fingerprint-trace`（plan-internal-skills-b04b，base 9e64edd5c）
执行：ZCode worker（builtin:bigmodel-coding-plan/GLM-5.3-Flash，attempt=1）
未执行任何 git commit / push / reset / stash；未改已应用迁移；未新增依赖。

## 1. 验收命令与结果（日志见 .agent/tmp/b04b/*.log）

| 验收 id | 命令 | 结果 | 日志 |
| --- | --- | --- | --- |
| db-knowledge | `cargo test -p screenpipe-db --lib knowledge` | exit 0，23 passed（含新增 input-hash 与 trace DB 单测） | test-db-knowledge.log |
| db-activity | `cargo test -p screenpipe-db --lib activity` | exit 0，17 passed | test-db-activity.log |
| engine-knowledge | `cargo test -p screenpipe-engine --lib knowledge` | exit 0，54 passed（含 8 个新测试：2 skill_revisions + 6 trace） | test-engine-knowledge.log |
| engine-activity | `cargo test -p screenpipe-engine --lib activity_ledger` | exit 0，22 passed | test-engine-activity_ledger.log |
| skill-routes | `cargo test -p screenpipe-engine --test internal_skill_routes` | exit 0，2 passed（8 条负例全部保持拒绝） | test-engine-internal_skill_routes.log |
| core-skills-pi | `cargo test -p screenpipe-core --lib skills && cargo test -p screenpipe-core --lib pi` | exit 0，4 + 405 passed（knowledge-fetch 114 行 ≤120、品牌中性） | test-core-skills-pi.log |
| check-three | `cargo check -p screenpipe-core -p screenpipe-engine -p screenpipe-db` | exit 0 | check-three.log |
| correctness | `cargo test -p screenpipe-db --test knowledge_correctness` | exit 0，8 passed（compute_input_hash 签名同步仅加实参） | test-db-knowledge_correctness.log |
| fingerprint-trace-map | 见 §2 / §3 表，逐条可定位；同窗口指纹一致有测试 `knowledge::trace::tests::same_window_reruns_share_one_result_fingerprint` | 通过 | 本报告 |
| diff_scope | `git status` 自查：改动全部落在 allowed_paths（含 `crates/screenpipe-engine/src/knowledge/` 目录内 answer.rs / migration.rs 的机械签名同步）；docs/archive、docs/plans、docs/reviews 为任务前已存在的未提交内容，未触碰 | 通过 | git status 输出 |

## 2. 指纹成分表（成分 → 代码位置）

核心：`compute_input_hash(sources, logical_scope, classification_revision, extractor_schema_version, prompt_version, skill_revision)`
位置：`crates/screenpipe-db/src/db/knowledge/types.rs:422`（fingerprint = sha256(hex)，`\x1f` 连接）。
skill 哈希定义：`crates/screenpipe-engine/src/knowledge/skill_revisions.rs:24` `skill_revision(body)` = sha256(include_str! 正文) 前 16 hex；正文用与 `cli/agent.rs` 相同的相对 `include_str!`，编译期嵌入，无运行时文件读取。

| 场景 | 成分序列 | 代码位置 |
| --- | --- | --- |
| WorkUnit 抽取（extract） | sources(uid:rev 排序) + scope + "activity-v1" + EXTRACTOR_SCHEMA_VERSION + EXTRACT_PROMPT_VERSION + **WORK_UNIT_SKILL_REVISION** | extract.rs:186（正式运行）、extract.rs:46（discover_and_enqueue）经 `input_hash_for`（sources.rs:271） |
| Compile 触发（extract 第 8 步入队） | 同上但 **KNOWLEDGE_DISTILL_SKILL_REVISION**（与 compile 处理器同族） | extract.rs:270 |
| 知识编译（compile） | work-unit id 集 + scope:type:title + 其余同族 + **KNOWLEDGE_DISTILL_SKILL_REVISION** | compile.rs:166 经 `input_hash_for` |
| 活动摘要运行时（summary_input_hash） | SUMMARY_PROMPT_VERSION + interval 身份 + 证据对 + 模型身份 + **ACTIVITY_SUMMARY_SKILL_REVISION + KNOWLEDGE_FETCH_SKILL_REVISION** | summarize.rs:477 |
| 活动摘要入队（discover_and_enqueue） | SUMMARY_PROMPT_VERSION + scope + **ACTIVITY_SUMMARY_SKILL_REVISION + KNOWLEDGE_FETCH_SKILL_REVISION**（与运行时同一口径） | summarize.rs:84 |
| 回答幂等键（answer，签名变化的既有调用点） | 空 sources + question + **KNOWLEDGE_FETCH_SKILL_REVISION** | answer.rs:384 |
| 历史回填抽取（migration，既有调用点） | 空 sources + scope + **WORK_UNIT_SKILL_REVISION** | migration.rs:295 |

单测：`db::knowledge::types::tests::skill_revision_is_part_of_the_input_hash`（skill 文本变化 → 指纹变化）；`knowledge::skill_revisions::tests::*`（16 hex、文本敏感、进 input_hash_for）。

## 3. 轨迹覆盖接口表（接口 → handler 位置）

通用逻辑：`crates/screenpipe-engine/src/knowledge/trace.rs` — `trace_admission`（处理前查 `knowledge_query_trace_count`，≥200 返回 429 `query_trace_limit_exceeded`，检查失败 fail-open）、`record_traced_read`（成功后落 `knowledge_record_query_trace`，失败仅记 error 日志不影响读）、`rows_fingerprint`（只对数据行序列化做 sha256，排除 `generated_at`/`time_range` 信封）。DB API：`crates/screenpipe-db/src/db/knowledge/trace.rs`；表：`crates/screenpipe-db/src/migrations/20260911160000_knowledge_query_trace.sql`（索引 `(trace_key, id)`）。

| 接口 | handler 位置 | 指纹输入（数据行） |
| --- | --- | --- |
| GET /activity-intervals | routes/activity_ledger.rs:278 `get_activity_intervals` | `intervals` 数组（含 history-clamp 早退分支记 `[]`） |
| GET /activity-intervals/missing-summary | routes/activity_ledger.rs:399 `get_activity_intervals_missing_summary` | `intervals` 数组 |
| GET /activity-intervals/:interval_id/evidence | routes/activity_ledger.rs:352 `get_activity_interval_evidence` | `evidence` 数组 |
| GET /search | server.rs:715 挂载点包 `knowledge::trace::traced_search`（handler 本体 routes/search.rs 未改，路由表结构未动） | 响应 JSON 的 `data` 数组（pagination/related 信封排除；非 JSON 格式记 Null） |
| GET /frames/:frame_id/text | routes/frames.rs:1585 `get_frame_text_data` | FrameTextResponse（frame_id + text_positions） |
| GET /frames/:frame_id/context | routes/frames.rs:1393 `get_frame_context` | FrameContextResponse（frame_id/text/nodes/urls/text_source） |
| GET /knowledge/knowledge | knowledge/routes.rs:368 `list_knowledge` | `items` 切片数组 |
| GET /knowledge/work-units | knowledge/routes.rs:82 `list_work_units` | `work_units` 数组 |
| GET /knowledge/work-units/:id | knowledge/routes.rs:130 `get_work_unit` | work_unit + evidence + related_knowledge 对象 |
| GET /knowledge/status | knowledge/routes.rs:1128 `knowledge_status` | KnowledgeStatusDto 整体 |

不带 header：`TraceAdmission::Untraced`，不记录、不限流（测试 `no_header_means_no_records_and_no_limit`）；轨迹写失败不吞读（`trace_write_failure_is_isolated_from_the_read`）；429 在处理前返回（`trace_key_over_the_limit_is_rejected_with_429`）；同窗口重跑指纹一致 / 不同窗口不同（`same_window_reruns_share_one_result_fingerprint` / `different_windows_get_different_fingerprints` / `envelope_fields_never_enter_the_fingerprint`）。

## 4. 偏差与残留风险

1. **测试层级偏差**：计划允许"临时库直调 handler 或既有测试 server 模式"，但 `AppState` 汇聚 AudioManager/VisionManager 等约 30 个字段，无法在单测中构造，故 trace 语义测试直接驱动 handler 所用的同一对函数（`trace_admission` + `record_traced_read`）+ 真实临时库，四个要求的语义均覆盖；HTTP 层未做端到端断言。
2. **计划外的机械签名同步**：`compute_input_hash`/`input_hash_for` 签名变化波及 answer.rs:384、migration.rs:295（计划 §2 未列这两个调用点）。均在 allowed_paths（`knowledge/` 目录）内；answer 传 knowledge-fetch 哈希、migration 传 work-unit 哈希，与对应场景语义一致。
3. **无新增依赖**：hex 未在 engine 直连依赖中，skill 哈希改用手写 `{:02x}` 编码，语义不变。
4. **语义影响面**：skill 正文变化（含本批 SKILL.md 追加）使四个 skill 的 revision 改变 → 存量 extract/compile/summarize 的旧 input_hash 不再命中幂等键 → 触发一次重算/重编译。这正是 D-16 预期，不迁移旧哈希。
5. **/knowledge/status 指纹含易变仪表**（oldest_waiting_ms 等）：status 无"数据行"概念，按整 DTO 记录；同窗口两次重跑该接口的指纹可能漂移，轨迹仍完整可审计。其余 9 个接口只对稳定数据行计算。
6. **/search 非 JSON 格式**（csv/tsv）：traced_search 无法解析数据行时按 `Null` 记录指纹（0 行）；JSON 默认路径不受影响。
7. **trace 表清理**：200/trace_key 天然有界，但 key 数量无上限；定时清理属明确非目标，建议后续 Cleanup job 按 `created_at` 清老 key。
8. **429 fail-open 例外**：上限检查本身 DB 报错时放行该读并记 error 日志（宁可少限流不可误伤读）；正常路径上限严格在处理前执行。
