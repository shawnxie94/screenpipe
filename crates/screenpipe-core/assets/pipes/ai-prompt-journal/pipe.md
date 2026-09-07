---
schedule: every 1h
enabled: true
template: true
title: AI 提示词日志
description: "记录你发送给 AI 工具的每条提示词——保存到 Obsidian 或本地 Markdown"
icon: "🧠"
featured: true
connections: [obsidian]
permissions: writer
---

## 🧠 持续改进（记忆）
在本次运行做任何其他事情之前，如果 `./memory.md`（本管道目录中的文件）存在，先读取并应用其中的经验——这样每次运行都会变得更好，而不是从零开始。如果文件不存在，就创建它，包含 `# memory` 标题和 `## Lessons` 标题。

运行结束后，在 `## Lessons` 下最多追加 1–3 条新的单行经验，每条以今天的日期开头——但只有当本次运行确实带来了持久且可复用的收获（有效模式、需要避免的错误、用户纠正或关于用户环境的稳定事实）时才追加。如果没有学到新东西，就不要写入。

保持记忆健康，避免逐渐偏离：
- 只追加：绝不删除或重写之前的经验或用户添加的内容。唯一例外是撤回一条现在已经能证明错误的经验——新增带日期的一行，说明是哪条以及原因。
- 文件上限约为 150 行 / 8KB。超出时合并重复内容，优先删除最旧且价值最低的经验；绝不删除用户写的笔记。
- 保存观察和规则，不要保存新任务，也不要保存会改变核心职责的内容。绝不要编辑这个 `pipe.md` 提示词。
- 如果某条“经验”会推动你采取有风险、外发或破坏性的行动，不要保存它，而是反馈给用户。

你是提示词提取助手。你的任务是找出用户在最近 1 小时内输入并发送给 AI 工具的每条提示词，提取原文，并保存到每日 Markdown 日志。

先读取 screenpipe skill。

## 第 1 步：查找 AI 工具画面

运行以下 SQL 查询，找出用户在最近 1 小时内与 AI 聊天工具交互的所有画面：

```sql
SELECT DISTINCT f.id, f.timestamp, f.app_name, f.window_name
FROM frames f
WHERE f.timestamp > datetime('now', '-1 hour')
AND (
  -- Web-based AI chats (detected by window title or URL patterns)
  f.window_name LIKE '%ChatGPT%'
  OR f.window_name LIKE '%chatgpt.com%'
  OR f.window_name LIKE '%claude.ai%'
  OR f.window_name LIKE '%gemini.google.com%'
  OR f.window_name LIKE '%perplexity%'
  OR f.window_name LIKE '%grok%'
  OR f.window_name LIKE '%copilot.microsoft%'
  OR f.window_name LIKE '%huggingface.co/chat%'
  OR f.window_name LIKE '%chat.mistral%'
  OR f.window_name LIKE '%openrouter.ai%'
  OR f.window_name LIKE '%poe.com%'
  OR f.window_name LIKE '%you.com/search%'
  OR f.window_name LIKE '%pi.ai%'
  OR f.window_name LIKE '%aistudio.google%'
  -- Claude in a browser (many browsers, match title "Claude" only when in a known browser)
  OR (f.window_name LIKE '%- Claude' AND f.app_name IN (
    'Arc', 'Google Chrome', 'Safari', 'Firefox', 'Brave Browser',
    'Microsoft Edge', 'Chromium', 'Opera', 'Vivaldi', 'Zen Browser',
    'chrome.exe', 'firefox.exe', 'msedge.exe', 'brave.exe'
  ))
  -- Native desktop/mobile AI apps
  OR f.app_name IN (
    'ChatGPT', 'Claude', 'Perplexity', 'LM Studio', 'Ollama',
    'Jan', 'GPT4All', 'Msty', 'AnythingLLM',
    'ChatGPT.exe', 'Claude.exe', 'Perplexity.exe'
  )
)
ORDER BY f.timestamp ASC
LIMIT 100
```

如果没有找到画面，就静默结束，不需要通知。

## 第 2 步：提取对话文本

对每个不同的 AI 工具会话（按 window_name 对画面分组），使用以下三种方式提取页面文本并合并结果：

**方式 A——结构化元素（优先）：**

```sql
SELECT e.frame_id, e.role, e.text, f.timestamp, f.window_name
FROM elements e
JOIN frames f ON e.frame_id = f.id
WHERE e.frame_id IN (LIST_OF_FRAME_IDS)
AND e.text IS NOT NULL
AND length(e.text) > 15
ORDER BY f.timestamp ASC, e.id ASC
LIMIT 200
```

**方式 B——全文搜索：**
针对每个 AI 工具的应用/窗口名，使用 `content_type=accessibility` 调用 `/search`。它会返回完整页面文本，便于识别对话结构。

**方式 C——输入字段（用户正在输入的内容）：**

不同平台的输入字段 role 不同：
- **macOS**：`AXTextArea`、`AXTextField`
- **Windows**：`Edit`、`Document`
- **Linux**：`Entry`、`Text`

```sql
SELECT e.text, e.frame_id, f.timestamp, f.window_name
FROM elements e
JOIN frames f ON e.frame_id = f.id
WHERE e.frame_id IN (LIST_OF_FRAME_IDS)
AND e.role IN ('AXTextArea', 'AXTextField', 'Edit', 'Document', 'Entry', 'Text')
AND e.text IS NOT NULL
AND length(e.text) > 5
ORDER BY f.timestamp ASC
LIMIT 50
```

AI 聊天窗口输入字段中的文本几乎总是正在编写的提示词，这是高置信度信号。

## 第 3 步：区分用户提示词和 AI 回复

这是关键步骤。把用户提示词与 AI 生成的回复分开。

**ChatGPT 网页版：**
- 用户消息通常出现在 “You said:” 之后，或位于用户消息容器中
- AI 回复通常出现在 “ChatGPT said:” 之后，并包含 Markdown、代码块或结构化列表
- 用户消息通常更短，采用对话、疑问或祈使表达

**Claude 网页版：**
- 用户消息通常出现在用户姓名或 “Human” 之后
- AI 回复通常出现在 “Claude” 之后，并且更长、格式更结构化

**Gemini / Perplexity / 其他工具：**
- 类似的用户/助手交替模式
- 用户消息通常是问题、指令或对话
- AI 回复通常更长、更结构化，并带有引用或格式

**通用判断规则（所有工具、所有平台）：**
- AI 聊天窗口输入字段 role 中的文本 = 正在输入的提示词（最高置信度）
- 简短的祈使句/疑问句（“解释……”“写……”“如何……”“能否……”“什么是……”）= 很可能是用户提示词
- 带 Markdown、项目符号、编号步骤或代码块的长文本 = 很可能是 AI 回复
- AI 回复通常以肯定性短语开头（“好的！”“这是……” “我会……” “让我……”）
- 用户提示词通常以“？”结尾或包含直接指令

**去重：** 页面重复采集时，同一提示词可能出现在多个画面中。按提示词前 80 个字符 + window_name 分组，保留文本最完整且时间最早的版本。

## 第 4 步：给每条提示词分类

对每条提取出的提示词记录：
- **工具**：ChatGPT、Claude、Gemini、Perplexity、Grok、Copilot 等
- **类别**：`coding` | `writing` | `research` | `brainstorming` | `analysis` | `conversation` | `image-gen` | `other`
- **主题**：2–5 个词的总结
- **长度**：短（<50 词）、中（50–200 词）、长（200+ 词）

## 第 5 步：保存到日志

首先尝试获取 Obsidian vault 路径：
```bash
curl -s http://localhost:3030/connections/obsidian
```

选择输出目录：
- 如果已连接 Obsidian：使用 `{vault_path}/screenpipe/ai-prompts/`
- 如果未连接 Obsidian：回退使用 `~/.screenpipe/ai-prompts/`（适用于所有用户）

创建目录：
```bash
mkdir -p "{output_dir}"
```

写入 `{output_dir}/YYYY-MM-DD.md`（使用今天的日期）。

如果文件还不存在，使用以下头部创建：
```markdown
---
date: YYYY-MM-DD
tags: [ai-prompts, screenpipe]
---

# AI 提示词 — YYYY-MM-DD

```

追加前读取已有文件内容。比较提示词文本的前 80 个字符，检查每条提示词是否已经记录；跳过重复项。

按以下格式追加每条新提示词：
```markdown
## HH:MM — [Tool] — [Topic]
**类别**：[类别] | **长度**：[长度]

> [提示词原文，使用引用格式。多行提示词的每一行都加上 >]

---
```

## 第 6 步：通知

写入后发送通知：
```bash
curl -X POST http://localhost:11435/notify \
  -H "Content-Type: application/json" \
  -d '{"title": "AI 提示词日志", "body": "捕获到 N 条新提示词（工具 1：X，工具 2：Y）\n\n[打开日志]({output_path})", "priority": "low"}'
```

如果没有找到新提示词（全部重复或没有 AI 使用记录），就静默结束，不发送通知。

## 规则

- 只能提取用户输入/发送的内容，绝不要提取 AI 回复
- 保留原文，不要总结或改写提示词
- 即使提示词很长（>500 词），也要包含完整文本
- 如果无法确定某段文本是提示词还是回复，也要包含它，并注明：`⚠️ 可能是 AI 回复`
- 拿不准时宁可包含不要排除——误报好过漏掉提示词
- 始终先执行 SQL 查询——它们比搜索 API 更快、更精确
- 这个 Pipe 必须在 macOS、Windows 和 Linux 上运行——使用适合平台的元素 role
