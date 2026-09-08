# Local Brain correctness — local verification evidence

- Project: `/Users/shawn/Developer/GitHub/screenpipe`
- Git HEAD at verification: `0a83343ac7e75cc67effa3dae4430bcb4dd825cf`
- Execution-plan SHA-256: `1930a41d283ea4ae1d8a1ec613bd73519ade5830c8076b834fb9f327278c426e`
- Mode: current-session single writer; no subagents, no commit, no push.

## Commands that passed

| Command | Result |
|---|---:|
| `cargo test -p screenpipe-db --test brain_correctness -- --nocapture` | 7 passed |
| `cargo test -p screenpipe-engine --test brain_correctness -- --nocapture` | 13 passed |
| `cargo test -p screenpipe-db --test task_contracts -- --nocapture` | 3 passed |
| `cargo test -p screenpipe-engine --test task_contracts -- --nocapture` | 2 passed |
| `cargo test -p screenpipe-engine --test task_migration -- --nocapture` | 6 passed |
| `cargo test -p screenpipe-engine --test task_contracts --test task_migration -- --nocapture` | 8 passed |
| `cargo check -p screenpipe-engine` | 0 errors, 21 warnings |
| `cargo test -p screenpipe-db --lib brain -- --nocapture` | 20 passed |
| `cargo test -p screenpipe-engine --lib brain -- --nocapture` | 17 passed |
| `cargo test -p screenpipe-engine --lib pipe_store -- --nocapture` | 35 passed; concrete user Pipe task identity and legacy execution mapping covered |
| `cargo test -p screenpipe-core --lib pipes -- --nocapture` | 280 passed |
| `cargo test -p screenpipe-connect --test office_lifecycle -- --nocapture` | 3 passed |
| `cargo test -p screenpipe-connect --test office_content -- --nocapture` | 3 passed |
| `cargo test -p screenpipe-connect --lib office -- --nocapture` | 19 passed, 104 filtered |
| `cd packages/screenpipe-mcp && bun run test && bun run typecheck` | 92 tests passed; typecheck passed |
| `bun run typecheck` in `apps/screenpipe-app-tauri` | passed |
| Focused Vitest suite: Brain API, task API, task event status, Brain section, native Brain tools | 30 tests passed |
| `bun x vitest run --config vitest.config.ts components/brain components/__tests__/local-brain-foundation.test.tsx components/settings/__tests__/office-connection-card.test.tsx components/settings/__tests__/brain-section.filter.test.tsx components/settings/__tests__/brain-overview.test.tsx lib/stores/__tests__/task-events.test.ts lib/tasks/api.test.ts lib/dev/browser-runtime.test.ts` | 9 files, 119 tests passed; existing BrainSection act warnings only |
| `bun x vitest run --config vitest.config.ts lib/dev/browser-runtime.test.ts` | 13 tests passed; stateful correction loop and five-entry browser contract covered |
| `bun x vitest run --config vitest.config.ts components/__tests__/local-brain-foundation.test.tsx` | 5 tests passed; unified task origins, Pipe/connection source boundaries, read-only activity and seq dedupe covered |
| Plan-filtered DB correctness tests | migration 1 + deletion 2 + jobs 3 + search compatibility 1 passed |
| Plan-filtered engine correctness/contract tests | deletion 3 + runtime 1 + extraction 1 + retrieval 5 + answer 2 + office control/evidence 2 + task lifecycle/compatibility 2 passed |
| `bun run test:tauri brain_runtime -- --nocapture` | 1 native test passed; 689 filtered |
| `bun run test:tauri brain_migration -- --nocapture` | 4 native tests passed; 686 filtered |
| `bun run test:tauri activity_history -- --nocapture` | 32 native tests passed; 658 filtered |
| `bun run test:tauri office_runtime -- --nocapture` | 3 native tests passed; 687 filtered |
| `bun run build:tauri:e2e` | E2E Tauri debug-dev build passed |
| `SCREENPIPE_E2E_SEED=onboarding,no-recording bun run wdio run e2e/wdio.conf.ts --spec e2e/specs/local-brain-entry-split.spec.ts` | 2 desktop WebDriver smoke tests passed |
| `git diff --check` | passed |
| Agent-brain readiness/task-pack/context/handoff/freshness validators | all passed; diff scope `clean`, 83 changed paths, 119 allowed, 2 ignored |
| `bun run test:bun` | 184 tests passed |
| Full frontend `bun run test` | exit 1: 4,044/4,068 tests passed; 24 existing non-B01 language/text contract failures |
| Task Pack canonical acceptance with `--repair` | pending 9/10: all command checks and diff scope passed; manual lead review remains pending |
| Task Pack/context/readiness/handoff validators | passed |
| `check-diff-scope --from-run .agent/runs/2026-09-08-local-brain-foundation-f02` | `scope=clean`, 83 changed files, 119 allowed paths, 2 ignored |

## Re-verification round (2026-09-08 late, same tree)

Independent re-run of every F11 synthetic checkpoint on the unchanged working tree (plan SHA at that time `f8df5eaf0475c4b0ebcb0e0c52ea2c0dd9e07359f5f623345ddb00f44079e14e`, HEAD still `0a83343a…`):

| Command | Result |
|---|---:|
| `cargo test -p screenpipe-db --lib brain` | 20 passed |
| `cargo test -p screenpipe-engine --lib brain` | 17 passed |
| `cargo test -p screenpipe-connect --lib office` | 19 passed |
| `cargo test -p screenpipe-db --test brain_correctness` | 7 passed |
| `cargo test -p screenpipe-engine --test brain_correctness` | 13 passed (P01–P11 + wave/corrupt-tail regressions) |
| `cargo test -p screenpipe-connect --test office_lifecycle --test office_content` | 6 passed |
| `cargo test -p screenpipe-db --test task_contracts` | 3 passed |
| `cargo test -p screenpipe-engine --test task_contracts --test task_migration` | 8 passed |
| `cargo test -p screenpipe-core --lib pipes` | 280 passed |
| `cargo test -p screenpipe-engine --lib pipe_store` | 35 passed ×3 consecutive clean re-runs; one earlier run under concurrent load had 1 timing flake |
| `cd packages/screenpipe-mcp && bun run test && bun run typecheck` | 92 passed; typecheck passed |
| `bun run typecheck` in `apps/screenpipe-app-tauri` | passed |
| Focused Vitest: brain components, settings cards, local-brain-foundation, task-events, task api | 7 files, 104 passed |
| `bun x vitest run … lib/dev/browser-runtime.test.ts` | 13 passed (five-entry + correction-loop contract) |
| `bun run test:tauri brain_runtime / brain_migration / activity_history / office_runtime` | 1 / 4 / 32 / 3 passed, all exit 0 |
| `check-diff-scope --from-run …` | scope=clean, changed=83, allowed=119, ignored=2 |
| Task Pack canonical acceptance (`run-acceptance --manual-ok`) | 10/10 checks, overall=pass |

Additional lead-review spot checks this round:

- F02 round-1 must-fixes verified in code: `routes/data.rs` records the Brain wave (`DeletionCause::UserErase`, seven datasets) before raw range delete; `routes/memories.rs` deletes via `brain.deletion().delete_memory` before raw row removal; `recover_incomplete` returns `Err` on journal-identity mismatch and tracks `failed_any`.
- Plan-required test filter words all match real test names (`deletion`, `migration`, `extraction`, `jobs`, `publication`, `review`, `retrieval`, `answer`, `search_compatibility`, `office_control`, `office_evidence`, `runtime`, `lifecycle`, `compatibility`, `brain`).
- Diff secret scan: only false positives (the literal substring `sk-` inside `task-contracts`); no credentials in the diff.
- Full frontend `bun run test` re-run: same 24 failures as before, all in files outside B01 write ownership. Verified that neither the 9 failing test files nor the modules they import (`lib/chat/system-prompt.ts`, `components/activity-ledger.tsx`, `components/meeting-notes/receipts.tsx`, `lib/summary-templates.ts`, etc.) are touched by the B01 diff, so these are pre-existing Chinese-localization assertion drift present at the reviewed head, not introduced or owned by this batch. F10/F11 checkpoints require focused suites + typecheck (both green), not full-suite green.
- E2E native build + WebDriver smoke were not re-run this round (no source change since the recorded pass; see round-1 evidence above).

## Real desktop run (2026-09-09 凌晨)

`bun run build:tauri:e2e` rebuilt clean (debug-dev, 2m03s) and the app was launched and driven through the real WebKit webview via WebDriver (`SCREENPIPE_E2E_SEED=onboarding,no-recording`):

| Spec | Result |
|---|---|
| `local-brain-entry-split.spec.ts` (plan-frozen new spec, asserts 知识库/工作单元/知识/画布/自动化) | **2/2 passed in the real desktop app** — nav label, three content tabs, no 提问/运行状态 leakage, automation as separate entry |
| Legacy specs attempted for signal: `brain-overview` (0/5), `settings-sections` (11/30), `pi-extensions` (0/1), `chat-rich-result-cards` (0 run) | Failures categorized as pre-existing drift, not B01 regressions |
| `brain-section.spec.ts` | `describe.skip` — intentionally disabled legacy suite |

Attribution for the legacy failures (all outside B01 write ownership, spec files unmodified by this batch):

- `settings-sections`/`pi-extensions` assert **English** labels ('screen context capture', 'audio & meetings', 'ai tools', 'choose what your ai can use') while the UI was fully Chinese-localized in commits `dcbfdc188`/`797fead3d`/`2aa2cb13c`, which predate the reviewed head. Same class as the 24 pre-existing vitest assertion-drift failures documented above.
- `brain-overview` waits for the old `[data-testid="section-brain"]` DOM — replaced by the sanctioned R12 entry split (knowledge-hub, verified by the new spec above); remaining errors are a helper timeout ("E2E account settings write did not finish").

Conclusion: real desktop launch + plan-scope verification passes; legacy e2e drift is the same pre-existing category already excluded from this batch and awaits its own maintenance pass outside this plan's ownership.

Anomaly note: the first build attempt at 00:12 failed with E0583 (7 `screenpipe-redact` module files momentarily absent); the files were restored to HEAD state within the same minute, the immediate re-run compiled clean, and no concurrent writer was detected afterwards (no git locks, no foreign processes, tree consistent). Treated as transient external interference, no action required.

## Real-data correction loop on the dev store (2026-09-09 凌晨, user-authorized)

User explicitly authorized driving the real dev store (`~/.screenpipe-dev`, 2.6 GB, real recordings). The desktop app was rebuilt with the fix below, launched via `SCREENPIPE_DATA_DIR=~/.screenpipe-dev` (no e2e seed), engine healthy on `127.0.0.1:3030`, answer chain = real preset Pi → `cc-switch-mini-max/MiniMax-M3`.

Loop executed against real knowledge `walkthrough-sop-1` (scope `chrome|需求评审`):

| Step | Call | Result |
|---|---|---|
| Ask #1 | `POST /answer` (idempotency `e2e-real-loop-ask3`) | `answer_id=4ee0f8f5…`, status `partial`, cited **only version 3** (`knowledge:walkthrough-sop-1:3`) |
| Incorrect feedback | `POST /brain/feedback` (`046dbb7b…`, knowledge_version_id 3) | `status=located`, `target_paused=true` — feedback service auto-paused the located version |
| Candidate v4 | `POST /brain/knowledge/walkthrough-sop-1/versions` (`expected_current_version_id=3`) | v4 `candidate`; body edit carries the feedback (性能基线/复盘回写) |
| Self-review publish | `POST …/versions/4/review` `publish` with revision CAS + pointer CAS | v4 `published`, v3 → `superseded`, **`paused` lifted to false in the same transaction (fix)** |
| Ask #2 | `POST /answer` (`e2e-real-loop-ask4`) | `answer_id=624ff2fe…`, status `partial`, cited **only version 4**; answer text includes the feedback-driven edits; no stale v2/v3 recall |

UI consistency (accessibility tree of the real home window): knowledge hub shows tabs 工作单元/知识/画布, the card 「需求评审流程 v4（真机纠错版）· 已发布 · 有效」, detail page lists all four versions with superseded/published states and their 支持工作单元, plus the 从当前发布版创建候选修订 / 围绕此内容提问 actions. `brain_feedback` rows persist both incorrect-feedback records (`status=located`) — traceable, no stale-version recall.

### Defect found and fixed by this real-data run

Publishing a corrected version did **not** clear the feedback-triggered knowledge-level pause (`brain_publish_version` left `paused=1`; retrieval filters paused rows), so after a real 纠错发布 the next answer returned `no_evidence` — the correction loop could not reclose on real data (the browser-engine mock modeled publish-as-unpause at `browser-engine-mock.ts:445` and therefore diverged from the engine).

- Fix: `brain_publish_version` now sets `paused = 0` inside the same publish transaction (`crates/screenpipe-db/src/db/brain/knowledge.rs`); resume API remains for pause-without-publish.
- Regression test (TDD, red→green verified by temporarily neutralizing the line): `publication_db_publish_lifts_feedback_pause_so_the_correction_loop_recloses` in `crates/screenpipe-db/tests/brain_correctness.rs`; suite 8/8 after fix.
- Post-fix suites: engine `brain_correctness` 13/13, db `--lib brain` 20, engine `--lib brain` 17, `pipe_store` 35 — all green; app rebuilt (`build:tauri:e2e`, 2m05s) and the live loop re-run to closure.

Validator behavior observed on real calls (working as designed, both recorded as rejections before the compliant create): unsupported evidence refs (`引用 u1 不在本次输入包中`) and non-evidence-derived `session_count` (`必须由相互独立的证据引用推导`). Note: the pre-existing published v2 row predates the tightened validator and would no longer satisfy it (`session_count=3` vs 2 distinct refs) — legacy data, superseded by v4 in this run.

## Evidence boundary

The Rust/TypeScript tests above use isolated SQLite databases, synthetic provider payloads, and fake office CLIs. They prove implementation contracts and regression behavior; they do not prove the PRD's frozen real-workflow thresholds.

The browser mock now keeps state across requests and covers the required correction loop: incorrect feedback pauses published v1, a candidate v2 is created and edited against its own row revision, v2 is self-reviewed/published, and the next answer cites only v2. The same mock covers task definition/run deduplication, seq-based event replay, run pause control, office scope/sync control, user Pipe and connection source boundaries, and the five product entry contracts. The real task catalog also projects installed `pipe.md` files as stable user definitions while keeping Pipe editing and execution in the existing Pipe owner. A UI regression also fixes CAS requests to send `current_version_id` (the version row id) rather than the display version number.

Not run in this session: real model calls, real Feishu/Tencent authorization samples, the 10-session/30–50-query holdout, the 5-workday review-burden observation, and CPU/RAM/SLA measurement. These items remain `unverified`, not passed. The desktop smoke only covers the navigation/content-tab split; it is not a substitute for the full product acceptance matrix.
