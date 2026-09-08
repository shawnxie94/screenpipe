---
id: personal-brain-brand-retained-identifiers
type: review
status: current
created_at: '2026-09-07'
updated_at: '2026-09-07'
related:
- docs/prd/personal-brain-local-first.md
- docs/trd/personal-brain-local-first.md
- docs/plans/personal-brain-local-first-execution-plan.md
---

# 知迹品牌替换 · 具名保留清单（Screenpipe 残留的保留理由）

依据 PRD R11：所有前端残留的英文品牌用法必须归入具名保留项。以下为 U13 扫描
（app/ components/ lib/ public/，含 alt/aria/tooltip/通知/标题）后决定保留的
Screenpipe 字样及理由；未列入且未替换的产品品牌残留视为遗漏。

## 保留项

| 位置 | 保留文本 | 类别 / 理由 |
|---|---|---|
| 仓库头注释（所有源文件） | `screenpipe — AI that knows everything you've seen…` / `https://screenpipe.com` | 上游署名 / 代码头，PRD R11 明确保留 |
| CLI / 包 / crate 名 | `screenpipe`, `screenpipe-mcp@latest`, `screenpipe-app`, `screenpipe-engine` … | 技术标识：包名、crate 名、命令名不可改 |
| MCP 配置示例（Claude/Codex/LM Studio/Grok 等卡片） | `mcpServers: { screenpipe: … }`、`npx -y screenpipe-mcp@latest`、`lmstudio://add_mcp?name=screenpipe…` | 技术标识：MCP 服务名与注册协议 |
| 深链 / URL / storage key | `screenpipe://meeting/...`、`screenpipe://view?path=...`、`~/.screenpipe`、`screenpipe-settings`、`screenpipe:pipes-welcome-dismissed` 等 | 技术标识：scheme、路径、storage key |
| 生成绑定（lib/utils/tauri.ts） | `commands.stopScreenpipe/spawnScreenpipe` 等 | 生成代码，禁止手改 |
| 云账号说明（connections-section CloudServiceRetiredCard） | “Screenpipe 云账号” | 真实存在的产品（已下线的上游云服务），描述事实 |
| 结构化输出 prompt（brain-overview） | “source-backed Screenpipe APIs” | 技术上下文（喂给模型的 API 名称），非展示文案 |
| 发布者归属（pipe-store） | “official Screenpipe publisher”、“built by screenpipe team” | 上游归属：真实发布方标识 |
| 服务/进程指引（权限弹窗、系统设置） | 系统权限列表中的真实注册名 `screenpipe`；`quit and reopen screenpipe to restore screen capture`（系统通知原文） | 系统真实名称：PRD 要求权限指引指向真实条目 |
| 用户内容 / 窗口标题样例（测试夹具） | `window_title: "screenpipe pull request"` 等 | 用户历史内容，不可改写 |
| 技能文件名（remote-agent-card 下载） | `screenpipe-api-SKILL.md`、`screenpipe-cli-SKILL.md`、`screenpipe-second-brain.md` | 技术标识：交付文件名 |
| chat-sidebar harness 标识 | `harness = "screenpipe"` | 技术标识：会话来源枚举值 |
| e2e 断言（acp-onboarding-ux.spec） | `expect(text).toContain("screenpipe")` | 断言的是 CLI 输出（技术标识），非页面品牌 |

## 已替换面（产品展示 → 知迹）

启动/加载页字标与 tagline、错误页与首页空态 alt、欢迎使用通知、托盘图标
title/aria、覆盖层服务文案、通知面板字标、通知设置说明、通知注册表描述、
Live View 引导文案、OAuth 卡片隐私句（“知迹永远不会看到你的密码”）、LM Studio
连接说明、Agent 卡片产品引用、浏览器配对弹窗、设置预览样例、远程代理描述。
品牌展示常量统一来自 `apps/screenpipe-app-tauri/lib/brand.ts`。

## 关于/帮助中的中英文对照

需要说明项目关系的位置使用「知迹 · Screenpipe」（PRODUCT_BILINGUAL_NAME）。
