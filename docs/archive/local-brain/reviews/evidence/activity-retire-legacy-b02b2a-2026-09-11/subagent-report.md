# B02b-2a 交付报告：停用旧叙事自动生成（数据库摘要成为唯一在跑的生产者）

日期：2026-09-11　分支：zh-local　计划：docs/plans/activity-retire-legacy-narrative-generation-plan.md（status: approved）

## 1. 实际读取 / 修改的文件

读取：计划全文、`components/activity-ledger.tsx`、`components/activity-ledger.test.tsx`、`lib/activity-history-persistence.ts`、`src-tauri/src/activity_history.rs`（顶部）、`lib/utils/tauri.ts`（仅 grep 验证，未改）、`app/(main)/home/page.tsx`（仅 grep 确认消费方）。

修改（共 4 个文件，+121 / −627）：

| 文件 | 改动 |
| --- | --- |
| `components/activity-ledger.tsx` | 删除生成触发链与专用状态、UI 收口（详见 §2） |
| `components/activity-ledger.test.tsx` | 改写/删除生成相关用例（详见 §3） |
| `lib/activity-history-persistence.ts` | 仅顶部加 1 行 legacy 注释（B02b-2a 停用自动生成、物理下线门禁） |
| `src-tauri/src/activity_history.rs` | 仅模块文档加 2 行 legacy 注释（同上）；生成器代码/测试/命令面零改动 |

## 2. 组件删除内容（activity-ledger.tsx）

- **生成回调**：`generateHistory`（内部调用 `commands.generateActivityHistory`，含全部错误映射逻辑）、`regenerateSelectedRange`、`addRecentActivity`（补洞触发）整体删除；`enableActivities` 重写为纯设置开关（只 `updateSettings({activitiesEnabled:true})`，不再接生成）。
- **补洞调度**：`recentRange` memo、`recentActivityAvailable`、`recentActivityDisabled`、`recentEligibilityTick` 状态、补洞解锁定时 effect、模块函数 `recentActivityUnlockDelay` 删除。纯函数 `canAddRecentActivity` 保留导出（helper 测试仍覆盖其 coverage 数学）。
- **生成中专用状态**：`historyLoading`/`setHistoryLoading`、`historyAbortRef`、`historyLoadingRef`、`GenerationSource` 类型、`noActivityMessage` 函数删除；快照 effect 中对应的 abort/复位行与"生成必须活得比页面久"的过时注释一并删除。
- **UI**：头部"刷新历史记录"按钮删除；"你可以离开此页面…"生成中横幅删除；两处 `historyLoading` 骨架分支删除；启用卡片文案"立即生成此时间范围…"→"开启后自动记录你的活动。"；启用后的空态从"生成活动"按钮卡片改为被动空态（"此范围内暂无活动记录。"）。
- **保留**：`loadHistorySnapshot`（DB 摘要优先、KV 兜底，B02b-1 语义不变，注释更新为 B02b-2a 已收口）、`activity-history-updated` 监听、legacy 激活 effect、AI 预设选择器与 range 控件、`historyCoverage`（legacy 激活判据仍用）。
- 卡加载风险：空数据时路径为 `cacheReady=false` 骨架 → 快照 resolve 后 `cacheReady=true` → 被动空态，无任何等待生成的分支（用例 1606 改写版覆盖）。

## 3. 测试改写（activity-ledger.test.tsx）

- helper：`generateActivities()`（点"生成活动"按钮）删除，替换为 `mockStoredHistory()` / `openLedgerWithStoredHistory()`（KV 快照渲染路径）；beforeEach 中旧生成器的 `generateActivityHistory` mockImplementation 删除（mock fn 保留，供"不再调用"断言）。
- **改写为"不再调用"断言**（12 处 `generateActivityHistory not.toHaveBeenCalled`）：1508/1541 legacy 激活（补断言）、1562→"enables activities without starting a generation"、1586→"enables activities without surfacing a generation failure"、1606→"waits for the encrypted cache lookup without offering generation"（同时断言空态不卡加载）、1636→"keeps the default preset and never generates on its own"、1715→"does not generate when capture starts after Activity opens"、1755→"keeps the empty state after a previously empty covered range"、1775→"does not regenerate uncovered time from the header"（刷新按钮不存在）、2050（断言翻转）、2132→KV 渲染 repaired 文档、2173→"renders the stored document without a backend generation"、2188（刷新按钮断言翻转为不存在）。
- **删除**（旧行为已无载体，共 8 个）：1841 coding agent 错误文案、1862 额度错误文案、1890 空录制范围文案、1917 慢生成两分钟、1954 生成 funnel 空壳、2003 卸载后在飞生成、2028 离开页面时生成继续、2157 quality failure 文案。对应被删代码：generateHistory 错误映射、生成中状态。
- **保持不动**：DB 优先/KV 兜底三个用例（1357/1385/1414）、`activity-history-updated`（1475）、1962 已有"不重新生成"用例、其余渲染/预览/图标用例（仅把 setup 从点击生成换成 KV 快照）；**既有两个失败未触碰**（1248 未改；2359 仅换 setup，`prompt: stringContaining("Draft a focused SKILL.md")` 断言原样保留，仍以相同方式失败）。

## 4. 验收命令与原始结果

| # | 命令 | 结果 |
| --- | --- | --- |
| 1 | `bun run typecheck` | exit 0（`bun x tsc --noEmit` 无输出）。注释微调后复跑仍 exit 0 |
| 2 | `bunx vitest run components/activity-ledger.test.tsx lib/activity-history-persistence.test.ts` | `Test Files 1 failed | 1 passed (2)`，`Tests 2 failed | 52 passed (54)`。失败项恰为基线两个：`activity history helpers > makes recorded meetings mandatory interpretation anchors`、`ActivityLedger > can draft a skill from every activity interval`。后者失败点为 showChatWithPrefill 断言不匹配（英文断言 vs 中文化实现，与基线同性质；已单跑确认 setup 正常渲染、spy 有调用、仅 ObjectContaining 不匹配）。基线对照：B02b-1 记录 62 用例 2 failed；本批删 8 个 → 54 用例，失败仍且仅是那两个 |
| 3 | `bun run bindings:check` | exit 0，`test specta_bindings::tests::tauri_bindings_are_current ... ok`；`git diff lib/utils/tauri.ts` 为空（未改动） |
| 4 | `grep -rn "generateActivityHistory" components lib` | 命中仅 `components/activity-ledger.test.tsx`（18 处，全部为 not-called 断言/mock 声明）与 `lib/utils/tauri.ts:447`（Tauri 生成物绑定）。**说明**：计划 §4 写"仅测试文件命中"，但 §3.3 同时要求保留 Rust 命令且禁止改 `lib/utils/tauri.ts`，生成物中的命令声明因此必然命中——按"不调用即无害 + tauri.ts 零改动"口径视为达标；组件与 lib 业务代码内零命中（我曾写入的注释因含该标识符已改写措辞） |

## 5. 未做 / 未验证项与风险

- **未做**（按边界）：未删 Rust 生成器/提示词/测试（B02b-2a 仅保号，物理下线待真实模型验收）；未改 `lib/utils/tauri.ts`、`crates/**`、`docs/**`、KV 数据结构；未修两个既有失败；未 commit/push；真实库只读、未调真实模型。
- **未验证**：真机端到端（无 DB 摘要且无 KV 的区间显示被动空态）仅由组件测试覆盖，未跑真实 app；Rust 侧仅注释改动，未重编原生（bindings:check 已触发一次 cargo test 且通过，间接覆盖编译）。
- **风险**：① 启用按钮不再即时产出内容，新用户开启后看到空态、需等 B02a Summarize 后台产出并通过 `activity-history-updated` 刷新——这是本批预期行为；② KV coverage 冻结在旧生成器停止处，legacy 激活判据（coverage 非空）只对老用户成立，新用户走显式 `activitiesEnabled` 设置，行为与改动前一致；③ 若需回滚，恢复 `activity-ledger.tsx` 被删的生成链即可（单文件、单提交）。
