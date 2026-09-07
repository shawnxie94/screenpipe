# screenpipe MCP 服务

这个包把本机运行的 screenpipe 暴露为 MCP 服务，让兼容 MCP 的 AI 工具可以查询个人屏幕历史、音频转写、活动记录和记忆。它是本地知识库的访问适配层，不负责托管数据，也不直接打开 SQLite 文件。

## 前置条件

- screenpipe 引擎正在本机运行，默认 API 地址为 `http://127.0.0.1:3030`。
- Node.js 18 或更高版本；从源码开发时使用仓库约定的 `bun`。
- macOS 的屏幕、麦克风和 Accessibility 权限已按需授予。

## 从源码安装

```bash
cd packages/screenpipe-mcp
bun install
bun run build
```

在 Claude Desktop 或其他 MCP 客户端中配置 stdio 服务。将路径替换为本地仓库的绝对路径：

```json
{
  "mcpServers": {
    "screenpipe": {
      "command": "node",
      "args": ["/绝对路径/screenpipe/packages/screenpipe-mcp/dist/index.js"],
      "env": {
        "SCREENPIPE_LOCAL_API_KEY": "本机 API 密钥"
      }
    }
  }
}
```

默认 stdio 模式适合与本机 AI 工具协作。若只在同一台机器上调试 HTTP transport：

```bash
bun run start:http -- --port 3031
```

HTTP 服务默认绑定回环地址，不应为了远程访问而把个人知识库暴露到网络。跨设备访问不属于当前项目目标。

## 可用工具

- `search-content`：查询屏幕文本、Accessibility、OCR、音频转写和输入事件。
- `activity-summary`：获取一段时间内的活动概览、应用分布和辅助上下文。
- `list-meetings` / `get-meeting`：查看会议记录和转写。
- `search-elements` / `get-frame-elements` / `frame-context`：查询结构化界面元素和指定帧上下文。
- `keyword-search`：对 OCR 与音频内容执行快速全文检索。
- `update-memory`：创建、修改和删除个人事实、偏好与决策记忆。
- `add-tags`：为帧或音频记录添加标签。
- `health-check`：检查本地采集服务和数据新鲜度。
- `list-audio-devices` / `list-monitors`：查看可用采集设备。
- `control-recording`：控制音频采集；不会暂停屏幕采集。
- `list-pipes` / `create-pipe` / `run-pipe` / `pipe-logs`：管理个人 Pipes。
- `export-video`：按时间范围导出本地屏幕记录。

工具的实际参数和返回结构以 `src/index.ts` 与本地引擎 API 为准。

## 示例问题

- “查找我今天在编辑器里处理过的数据库问题。”
- “总结过去一小时的会议转写。”
- “我上周对这个项目做过哪些决定？”
- “检查最近的本地采集是否正常。”
- “把这条信息保存为我的个人记忆。”

## 数据边界

MCP 服务只在本机转发工具请求：AI 工具 → MCP → `127.0.0.1:3030` → 本地 screenpipe 数据。录音、截图、OCR、转写和搜索结果不会由 MCP 服务另行保存。

如果用户把 AI 客户端配置为外部模型，客户端可能会把 MCP 返回的上下文发送给该模型服务；这属于 AI 客户端和 AI Preset 的配置边界，不应被描述为 screenpipe 的云端存储。

MCP 服务不应直接读写 `db.sqlite`、WAL 或共享内存文件。数据库生命周期、锁和恢复由 screenpipe 引擎负责。

## 测试

```bash
cd packages/screenpipe-mcp
bun run typecheck
bun test
```

更完整的本地 CLI、健康检查和检索流程见仓库内的 [screenpipe CLI skill](../../crates/screenpipe-core/assets/skills/screenpipe-cli/SKILL.md)。
