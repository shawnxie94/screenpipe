# screenpipe：本地信息收集口

screenpipe 是一个运行在本机上的信息收集与存取底座。它持续采集屏幕、音频和可访问性信息，并通过独立接入渠道（飞书、腾讯会议等）把其他应用的已有数据拉入同一个本地存储与检索底座。它保存事实和证据，不内建记忆、知识或研究产品——那些由下游应用和 AI Agent 通过本地 API / MCP / Pipes 取数自行实现。

它不是面向团队协作的 SaaS，也不是把个人数据上传到远端的云端记忆服务。

## 核心闭环

```text
屏幕 / 音频 / 可访问性 / OCR ──┐
                             ├──→ 本地统一存储与检索（SQLite / FTS / 文件）
飞书 / 腾讯会议 / 其他接入渠道 ─┘              ↓
                           外部应用 / Agent（REST API / MCP / Pipes）
```

screenpipe 负责保存事实和证据，AI 负责在用户配置的边界内进行整理、解释和生成。任何生成内容都应该能够回到对应的活动或原始记录，而不是只留下一个无法核验的结论。

## 当前定位

- **信息收集口**：统一接管本机信息的采集与外部数据接入，不做知识化加工。
- **个人使用**：服务单一使用者，不包含团队、组织、租户、ACL 或企业部署目标。
- **本地优先**：屏幕、音频、转写、活动记录和索引默认保存在本机。
- **AI 可替换**：通过 AI Preset 选择本地模型或兼容 API；模型调用产生的外部数据流由用户显式配置。
- **可被 Agent 使用**：通过本地 REST API、MCP 和 Pipes，让 AI 工具查询个人上下文。

## 已有能力

- 屏幕捕获、音频捕获，以及 Accessibility 优先、OCR 兜底的文本提取。
- 本地 SQLite 存储与全文检索，支持按时间、应用、窗口和内容类型查询。
- 活动 Ledger 与 Activity History：保存确定性的活动证据记录。
- 本地 REST API：默认监听 `127.0.0.1:3030`，提供搜索、帧、音频、健康状态和活动等接口。
- MCP 服务：供兼容 MCP 的本地 AI 工具查询屏幕历史、音频转写等。
- Pipes：用 Markdown 描述定时或手动运行的个人 AI 工作流，结果保存在本机。
- 飞书 / 腾讯会议接入：连接管理、授权与增量同步（独立 Connector，解耦后维护）。
- Tauri 桌面应用，以及可单独运行的 Rust CLI / 引擎。

历史方向（Local Brain 知识沉淀、个人工作台整合、zhiji 拆分）已整体退出当前范围，实现与规划文档归档在 [docs/archive/](docs/archive/)，不构成当前基线。

## 规划

统一产品与迭代入口见 SQLite Roadmap Artifact（`agent-brain roadmap show --project .`）：当前重点是移除历史知识域、把办公接入解耦为独立 Connector，然后扩展接入渠道并深化本地检索。screenpipe 只保留采集、接入、存储与检索能力，不内建记忆、知识库、研究综合或行动闭环。

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

如果需要启动已打包的本地 bundle，可从仓库根目录运行：

```bash
./launch-local.sh
```

脚本会比较源码指纹；bundle 缺失或不是当前源码构建时自动重新打包，
然后使用隔离的 `~/.screenpipe-dev` 数据目录和 `3130` API 端口启动。

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
  screenpipe-connect/       外部数据接入（office Connector、日历等）
  screenpipe-engine/        本地 API、调度与运行时
  screenpipe-db/            SQLite、迁移与检索
  screenpipe-core/          CLI、Pipes 与 Agent 能力
  screenpipe-semantic/      可选的应用语义解析
apps/screenpipe-app-tauri/  Tauri 桌面端与本地交互界面
packages/
  screenpipe-mcp/           MCP 服务
  sdk/                      SDK 与集成接口
docs/                       规划入口与架构文档
```

## 开发约定

- JavaScript / TypeScript 使用 `bun`，Rust 使用 `cargo`。
- 采集、编码和 SQLite 写入属于持续运行的热路径；改动时需要关注 CPU、内存、磁盘和数据完整性。
- 原生桌面开发使用 `apps/screenpipe-app-tauri` 中的脚本，不直接绕过仓库约定运行 Tauri 命令。
- 新需求先记录到仓库规划（`agent-brain roadmap show --project .`），再拆成可验证的实现步骤；不要用一次性脚本或临时文档代替长期设计。
- 测试和提交前检查见 [AGENTS.md](AGENTS.md)、[CONTRIBUTING.md](CONTRIBUTING.md) 和 [TESTING.md](TESTING.md)。

## 文档地图

- Roadmap：使用 `agent-brain roadmap show --project .` 查看当前产品方向、进行中事项与后续阶段。
- [ONBOARDING.md](ONBOARDING.md)：第一次参与开发时的安全注意事项与环境准备。
- [AGENTS.md](AGENTS.md)：仓库级协作、构建和验证规则。
- [DESIGN.md](DESIGN.md)：桌面端视觉与交互设计约束。
- [docs/archive/](docs/archive/)：已退出范围的历史方向决策记录（Local Brain、zhiji）。

本仓库是一个本地信息收集底座的实验场。文档优先描述当前代码和已经确认的方向；已经完成、失效或与本地个人目标冲突的方案不再作为现行说明保留。