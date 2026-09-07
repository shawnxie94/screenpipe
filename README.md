# screenpipe：本地优先的个人知识库

screenpipe 是一个运行在本机上的个人工作记录与知识沉淀底座。它持续采集屏幕、音频和可访问性信息，把这些原始证据保存在本地，再通过检索、记忆和 AI 工具把经历转化为可回看的个人知识。

它不是面向团队协作的 SaaS，也不是把个人数据上传到远端的云端记忆服务。当前仓库的目标是：先把一个人的工作轨迹可靠地留在自己的机器上，并逐步编译成可检索、可引用、可维护的个人知识库。

## 核心闭环

```text
屏幕 / 音频 / 可访问性 / OCR
                ↓
        本地 SQLite 与文件存储
                ↓
     活动记录 / 记忆 / 原始证据检索
                ↓
       AI 问答、复盘与知识沉淀
```

screenpipe 负责保存事实和证据，AI 负责在用户配置的边界内进行整理、解释和生成。任何生成内容都应该能够回到对应的活动或原始记录，而不是只留下一个无法核验的结论。

## 当前定位

- **个人使用**：服务单一使用者，不包含团队、组织、租户、ACL 或企业部署目标。
- **本地优先**：屏幕、音频、转写、活动记录、记忆和索引默认保存在本机。
- **证据优先**：先保留可回看的原始记录，再生成活动叙事和结构化知识。
- **AI 可替换**：通过 AI Preset 选择本地模型或兼容 API；模型调用产生的外部数据流由用户显式配置。
- **可被 Agent 使用**：通过本地 REST API、MCP 和 Pipes，让 AI 工具查询个人上下文。
- **持续沉淀**：目标不是一次性总结，而是把重复工作逐步整理成个人 SOP、决策规则和异常处理手册。

## 已有能力

- 屏幕捕获、音频捕获，以及 Accessibility 优先、OCR 兜底的文本提取。
- 本地 SQLite 存储与全文检索，支持按时间、应用、窗口和内容类型查询。
- 活动 Ledger 与 Activity History：分别保存确定性的活动证据和面向人的活动叙事。
- `memories` 事实型记忆：支持通过 REST / MCP 管理，并可同步到用户明确指定的本地文件生态。
- 本地 REST API：默认监听 `127.0.0.1:3030`，提供搜索、帧、音频、健康状态、活动和记忆等接口。
- MCP 服务：供兼容 MCP 的本地 AI 工具查询屏幕历史、音频转写和记忆。
- Pipes：用 Markdown 描述定时或手动运行的个人 AI 工作流，结果保存在本机。
- Tauri 桌面应用，以及可单独运行的 Rust CLI / 引擎。

## Local Brain 演进方向

当前改造围绕 `docs/prd/personal-brain-local-first.md` 展开，按小步闭环推进：

1. 从活动区间和已有证据中抽取 `Work Unit`，记录任务、输入、动作、决策、例外和结果。
2. 把多个 Work Unit 编译为个人知识候选，第一批类型是 SOP、Decision Rule 和 Exception Playbook。
3. 建立个人审核流程：候选知识默认不发布，用户确认后才进入可用知识集合；修订产生新版本，保留来源和历史。
4. 将时间线、语义内容、记忆和知识统一为多路召回，并在回答中返回可回链的证据。
5. 完善删除传播和生命周期管理，删除原始记录时同步清理相关证据、知识版本与索引。

以下内容不属于当前目标：团队或组织知识库、云端数据同步、账号与计费、企业管理后台、自动发布高风险操作，以及新的模型推理基础设施。

## 数据与隐私边界

- 原始屏幕、音频、转写和本地索引写入 `~/.screenpipe`；开发版本默认使用隔离的 `~/.screenpipe-dev`。
- 本项目不把“本地优先”理解为“模型调用永远不出网”。如果 AI Preset 指向外部模型服务，发送给该服务的内容仅限模型调用所需的上下文，并由用户自行选择与承担其数据策略。
- 外部记忆 Provider 或文件同步属于显式配置的出口；未配置时不启用。
- 个人数据的删除、备份和迁移应由使用者掌握；不要把生产数据目录直接用于实验性开发。
- 详细授权范围见 [LICENSE.md](LICENSE.md)。

## 快速开始

### 本地桌面开发

在仓库根目录安装前端依赖，然后从桌面应用目录启动：

```bash
cd apps/screenpipe-app-tauri
bun install
bun run dev:tauri
```

只需要验证普通 React 布局时，可以使用不构建 Rust 的浏览器模拟环境：

```bash
cd apps/screenpipe-app-tauri
bun run dev:web
```

浏览器模拟默认使用内存数据，不会访问真实采集数据库。原生开发构建、数据隔离和 TCC 权限说明见 [apps/screenpipe-app-tauri/README.md](apps/screenpipe-app-tauri/README.md) 与 [docs/macos-dev-builds.md](docs/macos-dev-builds.md)。

### 构建并运行 CLI

```bash
cargo build --release --features metal
./target/release/screenpipe record \
  --port 3030 \
  --data-dir "$HOME/.screenpipe-dev"
```

首次运行需要按操作系统授予屏幕录制、麦克风和 Accessibility 权限。停止进程后，再使用本地 API 或 MCP 查询已采集内容。

### 查询本地 API

```bash
curl 'http://127.0.0.1:3030/health'
curl 'http://127.0.0.1:3030/search?q=项目复盘&content_type=all&limit=10'
```

CLI 与 MCP 的完整用法见 [crates/screenpipe-core/assets/skills/screenpipe-cli/SKILL.md](crates/screenpipe-core/assets/skills/screenpipe-cli/SKILL.md) 和 [packages/screenpipe-mcp/README.md](packages/screenpipe-mcp/README.md)。

## 代码结构

```text
crates/
  screenpipe-capture/       屏幕与音频采集
  screenpipe-engine/        本地 API、调度与运行时
  screenpipe-db/            SQLite、迁移与检索
  screenpipe-core/          CLI、记忆、Pipes 与 Agent 能力
  screenpipe-semantic/      可选的应用语义解析
apps/screenpipe-app-tauri/  Tauri 桌面端与本地交互界面
packages/
  screenpipe-mcp/           MCP 服务
  sdk/                      SDK 与集成接口
docs/prd/                   当前 Local Brain 产品方向
```

## 开发约定

- JavaScript / TypeScript 使用 `bun`，Rust 使用 `cargo`。
- 采集、编码和 SQLite 写入属于持续运行的热路径；改动时需要关注 CPU、内存、磁盘和数据完整性。
- 原生桌面开发使用 `apps/screenpipe-app-tauri` 中的脚本，不直接绕过仓库约定运行 Tauri 命令。
- 新需求先记录到仓库规划，再拆成可验证的实现步骤；不要用一次性脚本或临时文档代替长期设计。
- 测试和提交前检查见 [AGENTS.md](AGENTS.md)、[CONTRIBUTING.md](CONTRIBUTING.md) 和 [TESTING.md](TESTING.md)。

## 文档地图

- [Local Brain PRD](docs/prd/personal-brain-local-first.md)：当前产品目标、边界和分期。
- [ONBOARDING.md](ONBOARDING.md)：第一次参与开发时的安全注意事项与环境准备。
- [AGENTS.md](AGENTS.md)：仓库级协作、构建和验证规则。
- [DESIGN.md](DESIGN.md)：桌面端视觉与交互设计约束。
- `docs/`：仍在维护的架构规格、数据库恢复和原生开发说明。

本仓库是一个个人本地知识库实验场。文档优先描述当前代码和已经确认的方向；已经完成、失效或与本地个人目标冲突的方案不再作为现行说明保留。
