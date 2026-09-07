---
id: research-personal-brain-local-first-preflight
type: research
status: partial
created_at: 2026-09-07
updated_at: 2026-09-07
sources:
  - docs/prd/personal-brain-local-first.md
  - "本机源码、应用元数据、CUA 原生 AX、认证 Screenpipe API、合成 Pi/SQLite probe"
related:
  - docs/trd/personal-brain-local-first.md
  - docs/reviews/personal-brain-local-first-trd-readiness.yaml
---

<!-- screenpipe — AI that knows everything you've seen, said, or heard -->
<!-- https://screenpipe.com -->

# Local Brain 技术预研记录

> 这是一轮设计前可行性采样，不是首版质量验收。模型 probe 只使用合成证据；文档不保存真实聊天、客户文档、私人模型地址或密钥。原始临时日志不进入 git。

## 1. 环境与开发版启动

- 日期：2026-09-07；macOS 15.0，arm64。
- 仓库 HEAD：`58617738c05f631e3d96b81eb51edf08f0818471`；工作区另有大量前端改动，因此 HEAD 本身不足以代表本次运行源码，关键文件 hash 见 §6。
- 启动：在 `apps/screenpipe-app-tauri` 执行 `bun run dev:tauri`，经过系统 native build queue/cache；未 raw cargo/tauri、未清空缓存、未覆盖 target 配置。
- 首次失败：生成的 Tauri permissions 列表仍指向迁移前 `learn/project/screenpipe`。错误在 `build.rs` 读取旧绝对路径，不是采集或模型故障。
- 可恢复修正：仅替换 `target/debug-dev/build/tauri*/output` 与 `out/*-permission-files` 中旧根路径，共81份生成文件；permission 列表中的新目标存在才修改。原件备份在 `/tmp/screenpipe-tauri-path-repair/`，未修改产品源码。
- 重启成功，实际 Engine 监听 `127.0.0.1:3130`，应用标识 `screenpi.pe.dev`。初始 `localhost:3030` 无法连接不等于此开发实例未运行。
- 开发 watcher 后续因 `ui_monitor-aarch64-apple-darwin` / `entitlements.plist` 等产物变化重编译，一次查询在重启窗口 connection refused；随后健康与认证检索再次通过。短暂断开不能计为稳定性验收通过。
- 临时日志：`/tmp/screenpipe-trd-dev-start.log`。包含本机运行信息，仅供本地排障，不提交或上传。

最后一轮完整探测窗口：2026-09-07 19:05:00 至 19:24:14（Asia/Shanghai）。health 为 healthy / frame_status ok；该轮进程指标为44帧捕获、44帧写入、0帧丢弃。开发构建和短窗口不代表 release CPU/RAM、长时间采集或质量已达标。

## 2. 三款办公工具

| 工具 | 当前确认 | 尚未证实 |
|---|---|---|
| 飞书 | 桌面客户端，关于窗口 `v7.75.18（7499.203）`，bundle `com.electron.lark`。Native AX 包含独立 messenger 与 messenger-chat HTML subtree，可见中文正文、日期、链接与输入区 | 与 Screenpipe 捕获记录的逐项匹配、中文检索命中、精确来源/应用归属；不能把 Native AX 读取等同采集引擎 AX 成功 |
| WPS | `12.1.26046 (26046)`，bundle `com.kingsoft.wpsoffice.mac`。首页可读，文件选择器可打开；API 的 WPS 过滤返回 OCR 记录 | 文字文档可见正文。合成 txt 文件已在 `/tmp` 准备，但此次 UI 操作未成功载入正文，不能以首页/最近文件名称证明正文兼容 |
| 腾讯会议 | `3.45.3 (405)`，bundle `com.tencent.meeting`。已打开登录后的首页，可读日程结构 | 没有进行中的会议；没有真实会议音频、转写或说话人/时间回链样本 |

Native AX 是操作系统向应用测试工具提供的可访问性文本。Screenpipe 还有采样时机、前台识别、过滤、写入和检索步骤；前者有文本，不能跳过后者直接宣布支持。

飞书的侧栏预览与当前聊天正文在同一窗口同时出现。适配必须按 subtree 分区，否则容易把其他会话摘要归入当前聊天；相对日期和缺失发送者应保留未知/部分可见语义。这里只记录结构，不复制聊天内容。

### 2.1 认证 REST 样本

使用运行日志确认的3130实例，设置 `SCREENPIPE_LOCAL_API_URL`；认证通过 `SCREENPIPE_DATA_DIR=~/.screenpipe-dev bun x screenpipe@latest auth token` 取得，只在子进程内使用，不输出 token。未直接读 live SQLite。每次 search 均限制时间、limit≤10、fields 与 max_content_length。

| 查询范围（无 q） | API total / 本页行数 | 观察与限制 |
|---|---|---|
| app_name=飞书，all | 0 / 0 | 当前中文应用名过滤无样本；仍需核对 Lark 别名与前台采样，不能推断完全不支持 |
| app_name=WPS，all | 6 / 5 | 返回 OCR / WPS Office；属于首页/文件选择器探测范围，未验证正文 |
| app_name=腾讯会议，all | 0 / 0 | 后台首页不等于已采集会议内容 |
| audio | 0 / 0 | health 为 meetings-only / waiting_for_meeting；尚未触发会议录制 |
| accessibility | 145 / 10 | 类型 UI，但本页所投影的 app_name 为空；来源与归属必须继续核查 |
| ocr | 145 / 10 | 类型 OCR，本页 app_name 为空；不是三款办公正文准确归属证明 |

API 使用 fields 时返回扁平键，如 `content.app_name`，不是嵌套 `content.app_name` 对象；后续脚本应按真实响应读取。表中保留实际 total 与行数差异，不根据 total 伪造本页样本数。本轮没有对中文 q 的办公正文做逐项命中评测，≥90% 指标未测试。

### 2.2 最小补测清单

1. 飞书：固定一个已打开会话，标注3–5个可见消息/日期单元和相邻侧栏干扰；用精确时间窗口匹配 frame/UI来源，再用中文短语查询；核对 app_name 的实际别名。
2. WPS：本地合成文字文档，固定标题/决定/异常三类文本，成功显示后验证 AX/OCR→检索→回链。扩展名或首页文件名不可充当正文样本。此次选择器有 clipboard timeout / offscreen / 操作后无状态变化，未据此判定 Screenpipe 不支持 WPS。
3. 腾讯会议：在有实际音频的测试会议中验证 meetings-only 触发、audio chunk、转写时间；单独检查未识别说话人保持未知。当前零音频是采样条件未具备，不是转写正确率0的测量。

这些是进入对应兼容设计冻结的前置样本；后续正式验收仍需每款足够的标注分母和10会话质量集。

## 3. 指定 Pi 模型 probe

本机全局 Pi `0.85.0`，离线模型列表有 `shawnhub-copy / deepseek-v4-flash-0731`。在空临时工作目录执行：

```sh
pi --provider shawnhub-copy --model deepseek-v4-flash-0731 \
  --print --mode json --no-session --no-tools --no-extensions --no-skills \
  --no-prompt-templates --no-context-files --no-approve \
  --system-prompt '<只按证据抽取的中文系统指令>' '<两条合成证据>'
```

输入只包含：E1 要求离线查询；E2 首版选全文检索、向量检索后续评测；明确没有实施/测试结果。外层超时55s。

| 检查 | 实测 |
|---|---|
| 进程 / 耗时 | exit0，6.12s |
| 输出 | 合法 JSON，stop_reason=stop |
| 引用 | 所有引用属于输入 E1/E2，无新增来源 |
| 未知结果 | result=null，没有声称实施或上线成功 |
| 用量 | input252 / output135 / total387 tokens |
| 真实私人内容 | 未发送 |

输出证明一条合成请求可调用、按目标结构返回。它不证明全部事实语义支持、不证明取消实际回收、不证明应用内集成或真实质量。未把首轮“无费用限制”解释为无限单次输入/重试。

### 3.1 应用内差异

- core 固定 Pi / pi-ai 为0.84.1，与全局0.85.0不同。
- 应用有独立 `pi-config`，现有全局配置 seed 逻辑会复制更多文件，不能直接用它完成新的最小权限绑定。
- 当前 Preset enum 不含 `shawnhub-copy`；需映射到现有 Custom Preset，保留真实 provider/model 身份。该全局 provider 元数据使用 `openai-completions`，不把私人端点写入文档。
- 本次仅分类查看全局 credential 字段为 `literal_or_other`，未确认它已经是可用的 Keychain 引用，未复制/显示原值，也未修改全局配置。应用接入须建立 `kb-model` secret_ref 再测试，不能默认符合密钥约束。
- 应用已有 ephemeral side session 可 `--no-session`，但普通后台路径的扩展、工具、空结果重试、15分钟 timeout 与会话目录行为不足以直接满足本 TRD。
- 因此 DR-TRD-002 仍开放：需一条应用内小样本的成功/取消/超时/目录清理证据，同时证明不静默换模型、不读取项目上下文。

## 4. 中文检索实验

### 4.1 现有 tokenizer 的反例

本机 Python 内存 SQLite 3.43.2，文本“首版选择本地全文检索，向量检索后续评测。”：

| 查询 | unicode61 | trigram |
|---|---:|---:|
| 全文检索 | 0 | 1 |
| 检索 | 0 | 0 |

这是 tokenizer 语义实验，不是 Screenpipe 线上查询结果。现行 `frames_fts` 与文本归一化源码证实现有中文漏召回风险。[SQLite 官方 FTS5 文档](https://www.sqlite.org/fts5.html)是 tokenizer 行为与 trigram 限制的外部依据。

### 4.2 使用仓库已编译 SQLite 的投影实验

将既有 `debug-dev/build/libsqlite3-sys-556fc274496ffbb6/out/libsqlite3.a` 只读链接到 `/tmp` 的 probe 动态库，Python ctypes 仅打开 `:memory:`；没有编译/修改 app 源码、没有改变 native cache、没有访问用户数据库。SQLite 版本3.51.3；静态库 SHA-256：`e67f26eabc9afee205729a452ca9d7bd79a3896785f6884eebf0826e167c6456`。

投影规则为 CJK unigram/bigram + 拉丁词；FTS 找候选，原文 contains 再排除不同位置的 token 拼接。4条合成文档中专设一条包含分散“全文/文检/检索”的干扰项。

合成输入按 ID 顺序：

```text
1 首版选择本地全文检索，向量检索后续评测。
2 检索失败时保留未知。
3 前端使用 TypeScript 和 Rust。
4 文档的中间内容包含全文；下一段记录文检，再一段出现检索。
```

| 查询 | 预期 ID | 实际 | 结果 |
|---|---|---|---|
| 全文检索 | 1 | 1 | pass，排除了跨段拼接的第4条 |
| 检索 | 1,2,4 | 1,2,4 | pass，二字中文命中 |
| 检 | 1,2,4 | 1,2,4 | pass，单字有界检索 |
| Rust | 3 | 3 | pass |
| 不存在 | 空 | 空 | pass |

原文209字节，token投影795字节（约3.8倍）；该极小 fixture 库28,672字节，各查询<1ms。页开销占主导，不能将此体积/耗时外推到真实库。设计方向由此闭合，性能、归一化offset、代码符号/Unicode边界、真实容量仍是实施测试范围。

## 5. 结论与交接

- 已完成：源码责任边界核对；开发版启动恢复与健康/认证检索；三款版本/入口；全局指定 Pi 合成调用；现有中文 FTS 风险复现与投影可行性。
- 未完成：三款正文/真实会议采集闭环；应用固定 Pi 版本下 Keychain/profile/取消/会话清理；10真实会话与独立保留集；任何产品/性能达标结论。
- 对应 TRD：保留两个前置设计阻塞 DR-TRD-001 / 002，中文投影 DR-TRD-003 已有小样本证据。具体语料 DI-EVAL-001 须在质量评测与调参前冻结。

## 6. 关键源码与证据指纹

下表由本轮结束时的文件内容生成；若这些文件变化，应刷新影响分析和相关结论。前端和原生资源的其他并行修改不归本任务所有。

| 路径（相对仓库） | SHA-256 |
|---|---|
| `AGENTS.md` | `5e285bd461f88c7ad9e469f36addd9a9d33129b5b04f04cc1e91fd9e95f28542` |
| `VISION.md` | `bf2b38ef85a3642fb293d0bfc7b9960ced3e67ead8c7a9cad62986d30a42dac0` |
| `DESIGN.md` | `4cddbf7f906a5df7a1190a219c5911d6cc7d824a76c54f38ae63e9af1039fc92` |
| `Cargo.lock` | `cbbe5838d4902eba107f0e88e891e0c20f1a65b10244235f934b4e117ebb2f78` |
| `crates/screenpipe-db/src/db/mod.rs` | `59ddc6fce34511960a4a7e9521f35efb1aa3e2c209a333983f01d8f0734ebfe7` |
| `crates/screenpipe-db/src/db/activity_ledger.rs` | `7b2e20f6100fd4f35a5b6064d8f16781c7deb31d62686451c24727fd9a507bf6` |
| `crates/screenpipe-db/src/db/maintenance.rs` | `a8af48903619a97f82f9a2d97ff3959f9934cd2cd2a8b4f29058c7bd9b3c6e31` |
| `crates/screenpipe-db/src/db/search.rs` | `7439b0b898ee8e228de93e4dbf842786d1910928a21924e789a922f3072e5aab` |
| `crates/screenpipe-db/src/text_normalizer.rs` | `40996025041dda9e4e26f2830dc4374c45dfea32ec1b4df2b8ccf40ae889b573` |
| `crates/screenpipe-db/src/db/memories.rs` | `d73d036645e5cd8992d0f90211cfdc73090b88e072eb5cf2e09cd84acb59020a` |
| `crates/screenpipe-engine/src/local_chat.rs` | `8872d4e3b468f3feb49532b1b128105686263e105ca458710331764821519ea3` |
| `crates/screenpipe-engine/src/retention.rs` | `f730e9a3b8c6ea74d62eee4e5e91c4f55b882a94def00bddc5b633df44b9ee0f` |
| `crates/screenpipe-engine/src/routes/data.rs` | `ff18319326c66c9261d60de8ad820005607bcea4bc78f6d6240297f2dc544d4b` |
| `crates/screenpipe-engine/src/routes/retranscribe.rs` | `db0f91e7ef7344e894a28aac9ecddd22da7f2a99e08e908b72b8da36b8027172` |
| `crates/screenpipe-core/src/agents/pi.rs` | `e287332adef900c90cbbb21a2a23c73e36cb42b9c5b95cd36666ffe2ee06303d` |
| `apps/screenpipe-app-tauri/src-tauri/src/activity_history.rs` | `4841215ce5a589c04b59c4a881eea326f601f37d62532280355d598f3ca13758` |
| `apps/screenpipe-app-tauri/src-tauri/src/pi.rs` | `c9da57945c02e6e8bdc70cfcb192e68bc3ac26b55f0afc6871c115946e6dd188` |
| `apps/screenpipe-app-tauri/src-tauri/src/store.rs` | `493cf4193b8779c5b8aff0c79f148b014a2cf55e8e7d667a1ffbc764cadcaa7e` |
| `packages/screenpipe-mcp/package.json` | `925dae8783692d15e4112e912df8f31cbf4598f1e9777a6d13738defcc522318` |
| `packages/screenpipe-mcp/src/index.ts` | `becc383a9a34bbc8a49849e07cf1dadc3954c0e105fce26f380943db72d12efe` |
| `crates/screenpipe-semantic/src/registry.rs` | `821cfbe68aadb61a388502dce0558f4325add89d27341772611cba42efb4c8ea` |
