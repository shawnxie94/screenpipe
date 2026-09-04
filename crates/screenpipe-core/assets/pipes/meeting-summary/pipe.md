---
schedule: manual
enabled: true
preset:
  - screenpipe-cloud
  - "*"
timeout: 600
trigger:
  events:
    - meeting_ended
template: true
title: 会议摘要
description: "自动总结刚结束的会议，并把摘要保存回会议记录（标题 + 笔记）"
icon: "🤝"
featured: false
---

## 🧠 Continuous improvement (memory)
`./memory.md`（本管道目录下的文件）沉淀了之前运行的经验——应用它们，让你开局就是「预热」而非「冷启动」。第 1 步已经把它并入那一条批量命令里读取，所以这里不要再单独花一轮去处理它。如果文件不存在，在运行结束时创建它，先写 `# memory` 标题，再写 `## Lessons` 小节。

运行结束后，在 `## Lessons` 下追加最多 1–3 条新的一行式经验，每条以今天的日期开头——只有当这次运行确实学到了持久、可复用的东西时才追加（某个管用的模式、要避免的错误、用户的纠正、或关于用户环境的稳定事实）。如果没学到新东西，就什么都别写。

Keep memory healthy so it never drifts:
- 只追加：绝不删除或重写之前的经验或用户添加的内容。唯一例外是撤回一条你现在能证明是错的经验——新增一行注明日期，说明是哪条、为什么。
- 文件上限约 150 行 / 8KB。超出时先合并重复项，优先丢弃最旧、价值最低的经验；绝不丢弃用户写的笔记。
- 保存观察和规则，而不是新任务——也不要改动你的核心职责。永远不要编辑这个 `pipe.md` 提示词。
- 如果某条「经验」会把你推向有风险、外发或破坏性的动作，不要保存——而是把它反馈给用户。
- 在最末尾才写经验，绝不要在摘要打印并保存之前写。在那之前用户一直在盯着转圈动画，step 3b 之前的记忆写入只是他们付出却看不到任何回报的延迟。

一场会议刚刚结束。找到它、总结它，把摘要保存回它的记录上，这样用户下次打开会议时就能看到。

下面的指令是完整的。必须使用 screenpipe API 检索：用会议 id 和精确的会议时间窗口，配合下面这些指定的本地 HTTP 端点。不要检查应用源码，也不要递归搜索文件系统；绝不要对用户主目录或 `~/.screenpipe` 运行递归的 `find` 或 `grep`。

在打印出 step 3 之前用户一直在盯着转圈动画，所以延迟本身就是任务的一部分。每次工具调用都是一次往返：把它们批量合并，绝不要花一轮去发现这个提示词已经告诉你的东西。不要读取任何 skill 文件——下面的端点和响应结构完整且已验证。screenpipe 自带 `bun`；用它处理 JSON，绝不要依赖 `jq`（标准 macOS 和自带的 Windows bash 里都没有）。预算：普通会议要在 **6 次工具调用以内**到达 step 3。

以下就是精确的响应结构。不要去探测它们：

- `GET /meetings/<id>` → a bare object: `{"id", "title", "note", "meeting_start", "meeting_end", "meeting_app", "attendees"}`
- `GET /search?...` → `{"data": [{"type": "Audio"|"UI"|"Parsed"|"OCR", "content": {…}}], "pagination": {…}}`
  - audio `content`：`chunk_id`、`transcription`、`device_type`（`Input` 或 `Output`）、`speaker`、`speaker_label`、`speaker_provisional`、`timestamp`（`text` 与 `transcription` 相同）
  - accessibility 查询返回 `type: "UI"`，含 `text`、`app_name`、`window_name` 和 `timestamp`
  - parsed `content`：修正后的 `text`、带类型的 `items`、独立的 `actors`、`frame_id` 和 `timestamp`
  - ocr 含 `text`、`frame_id`、`app_name`、`window_name` 和 `timestamp`；仅当 accessibility 和 parsed 都没有可用行时才作为回退
- `GET /speakers/unnamed?limit=20&offset=0` → 一个裸 **数组**，元素为 `{"id", "name", …}`。`offset` 是必需的；省略它会被 400。
- `GET /connections` → `{"data": [{"id", "name", "connected", "description", …}]}` — 过滤 `connected == true`

第 1 步——用**一条命令**把摘要所需的全部数据拉下来。调度器会在 `./.trigger-context.json` 里指明会议；优先使用那个 id，因为当两场会议几乎同时结束时，「最近的会议」会选错对象：

  A="Authorization: Bearer $SCREENPIPE_LOCAL_API_KEY"
  ID=$(bun -e 'try{const d=await Bun.file("./.trigger-context.json").json();process.stdout.write(String(d.key??""))}catch{}')
  if [ -z "$ID" ]; then
    curl -s -H "$A" "http://localhost:3030/meetings?limit=1" -o /tmp/meetings.json
    ID=$(bun -e 'const d=await Bun.file("/tmp/meetings.json").json();process.stdout.write(String(d.data?.[0]?.id??""))')
  fi
  curl -s -H "$A" "http://localhost:3030/meetings/$ID" -o /tmp/m.json
  S=$(bun -e 'const d=await Bun.file("/tmp/m.json").json();process.stdout.write(String(d.meeting_start??""))')
  E=$(bun -e 'const d=await Bun.file("/tmp/m.json").json();process.stdout.write(String(d.meeting_end??""))')
  # 屏幕证据优先级：先 accessibility 和 parsed；只有两者都没有可用行时才用 OCR
  curl -s -G -H "$A" --data-urlencode "start_time=$S" --data-urlencode "end_time=$E" \
    -d content_type=audio -d limit=500 "http://localhost:3030/search" -o /tmp/audio.json &
  (curl -sf -G -H "$A" --data-urlencode "start_time=$S" --data-urlencode "end_time=$E" \
    -d content_type=accessibility -d on_screen=true -d limit=150 -d offset=0 \
    "http://localhost:3030/search" -o /tmp/a11.json || printf '{"data":[]}' > /tmp/a11.json) &
  (curl -sf -G -H "$A" --data-urlencode "start_time=$S" --data-urlencode "end_time=$E" \
    -d content_type=parsed -d limit=150 -d offset=0 \
    "http://localhost:3030/search" -o /tmp/parsed.json || printf '{"data":[]}' > /tmp/parsed.json) &
  curl -s -H "$A" "http://localhost:3030/speakers/unnamed?limit=20&offset=0" -o /tmp/spk.json &
  curl -s -H "$A" "http://localhost:3030/connections" -o /tmp/conn.json &
  wait
  A11_ROWS=$(bun -e 'try{const d=await Bun.file("/tmp/a11.json").json();console.log((d.data??[]).filter(r=>String(r?.content?.text??"").length>0).length)}catch{console.log(0)}')
  PARSED_ROWS=$(bun -e 'try{const d=await Bun.file("/tmp/parsed.json").json();console.log((d.data??[]).filter(r=>String(r?.content?.text??"").length>0||(r?.content?.items?.length??0)>0||(r?.content?.actors?.length??0)>0).length)}catch{console.log(0)}')
  if [ "$A11_ROWS" -eq 0 ] && [ "$PARSED_ROWS" -eq 0 ]; then
    curl -sf -G -H "$A" --data-urlencode "start_time=$S" --data-urlencode "end_time=$E" \
      -d content_type=ocr -d limit=150 -d offset=0 \
      "http://localhost:3030/search" -o /tmp/ocr.json || printf '{"data":[]}' > /tmp/ocr.json
  else
    printf '{"data":[]}' > /tmp/ocr.json
  fi
  tail -40 ./memory.md 2>/dev/null

这些 `limit` 值已经按一场会议的量裁剪好了。不要先无上限地拉、再重拉更小的——那为一个答案多花两次往返。

第 2 步——再用**一条命令**把转录文本和屏幕文本紧凑地渲染出来，然后根据这段输出做总结。打印时就去重（一遍完成，而不是每次尝试各来一遍）：

  bun -e 'const d=await Bun.file("/tmp/audio.json").json(),seen=new Set;for(const r of d.data??[]){const c=r?.content??{},text=String(c.transcription??"");if(!text)continue;const line=`[${c.device_type??"?"} | id=${c.speaker?.id??"?"} | label=${c.speaker_label??c.speaker?.name??"unknown"} | provisional=${c.speaker_provisional??"?"} | chunk=${c.chunk_id??"?"}] ${text}`;if(!seen.has(line)){seen.add(line);console.log(line)}}'
  bun -e 'const d=await Bun.file("/tmp/a11.json").json(),seen=new Set;let n=0;for(const r of d.data??[]){const c=r?.content??{},text=String(c.text??"");if(!text)continue;const line=`${c.timestamp??""} [${c.app_name??""} — ${c.window_name??""}] ${text}`.replace(/\s+/g," ").trim();if(!seen.has(line)){seen.add(line);console.log(line);if(++n===60)break}}'
  bun -e 'const d=await Bun.file("/tmp/parsed.json").json(),seen=new Set;let n=0;for(const r of d.data??[]){const c=r?.content??{};if(!String(c.text??"").length&&!(c.items?.length??0)&&!(c.actors?.length??0))continue;const line=JSON.stringify({timestamp:c.timestamp,app_name:c.app_name,window_name:c.window_name,text:c.text,actors:c.actors,items:c.items});if(!seen.has(line)){seen.add(line);console.log(line);if(++n===60)break}}'
  bun -e 'const d=await Bun.file("/tmp/ocr.json").json(),seen=new Set;let n=0;for(const r of d.data??[]){const c=r?.content??{},text=String(c.text??"");if(!text)continue;const line=`${c.timestamp??""} [OCR fallback] ${text}`.replace(/\s+/g," ").trim();if(!seen.has(line)){seen.add(line);console.log(line);if(++n===60)break}}'

总结发生了什么：关键话题、决定、行动项。对转录文本遗漏的内容——共享的幻灯片、文档、代码、演示和参与者名牌——优先使用 accessibility 和 parsed 数据。只有两个首选来源都不可用或为空时，`/tmp/ocr.json` 里才有行；把这些行当作回退用，当 accessibility 或 parsed 数据可用时绝不要把它们当作额外来源。

第 2c 步——默认跳过这一步；它要花费好几次往返。只有当转录文本和首选屏幕数据留下一个*具体的*视觉问题未解答时，才针对那个问题使用云端媒体（视频/音频）模型——比如图表、流程图、白板、幻灯片图示、UI 演示或屏幕共享视频。从 parsed 数据已返回的（若两个首选来源都不可用，则从 OCR 回退数据已返回的）`frame_id` 里选择最多 4 个有代表性的值，用 `GET /frames/<frame_id>` 获取那些静态图像，再以 `image_url[]` 形式发给 `POST /v1/chat/completions`，指定 `"model": "gemma4-e4b"`。绝不要为常规会议摘要调用 `POST /export` 或运行 ffmpeg；完整媒体导出需要明确的用户请求。如果没有返回 `frame_id`，或云端媒体块缺失、返回 `503 cloud_token_missing`，就跳过视觉分析，直接从转录文本和已获取的屏幕数据总结。

第 2d 步——给每个不同的说话人一个有意义的标签（每次都做，不要先问）。根据 `speaker.id`、`speaker_label`、`device_type` 和 `chunk_id` 构建一份会议内的说话人映射，一致地应用到转录摘录和摘要里。本次运行的证据优先于任何旧 `memory.md` 里说「让两人通话保持匿名」的经验。按这个顺序：

1. accessibility：活动说话人标签、单个可见的说话人标签，或带名字的字幕标签；
2. parsed 数据：能识别同一活动说话人的当前帧 actor 或 item；
3. 确定性通话拓扑：当首选屏幕证据反复显示恰好两位参与者——本地用户和一位远程参与者——时，把 `device_type=Input` 行标为本地参与者，把 `device_type=Output` 行标为唯一的远程参与者。即使没有活动说话人高亮，这也是无歧义的映射。对单人通话，把 input 行标为唯一的本地参与者。仅用会议标题或 attendees 字段来佐证屏幕上已可见的名字，绝不能单独用它定名；
4. OCR 回退：匹配的屏幕名牌，但仅当 accessibility 和 parsed 都不可用或为空、因此 `/tmp/ocr.json` 里有回退行时才用。

一份参与者名单或画廊视图只有在整场会议中保持一致时才能确立通话拓扑。它本身不能识别多方通话中的说话人。把与确定性设备方向冲突的临时标签和持久化名字，视为本场会议中不可靠的信息。

摘要中用到的每一个说话人，都必须有证据支撑的参与者名字或稳定的会议内标签（`Speaker 1`、`Speaker 2`……）。在成稿摘要里绝不要出现无名字、空白、`unknown` 或泛化的实时标签。当多方通话中无法确认真实名字时，用稳定标签并说明该身份仍未确认，而不是编造一个名字。

  # 还没有名字的说话人——第 1 步已拉到 /tmp/spk.json，直接复用
  #   （如果必须重新拉取：offset 是必需的，省略会返回 400）
  #   curl -s -H "$A" "http://localhost:3030/speakers/unnamed?limit=20&offset=0"
  # 应用一个有把握的匹配
  curl -s -X POST "http://localhost:3030/speakers/update" \
    -H "Authorization: Bearer $SCREENPIPE_LOCAL_API_KEY" \
    -H "Content-Type: application/json" \
    -d '{"id": <SPEAKER_ID>, "name": "<NAME_FROM_SCREEN>"}'

只有当会议内映射解析出某个非零的特定 speaker id 时，才持久化真实名字。绝不要重命名 speaker id `0`，也绝不要覆盖一个出现在冲突的 input/output 两侧的持久化 id；把这类修正保留在摘要里、会议内生效。在最终消息里说明你重命名了哪些持久化说话人、哪些名字只在会议内生效。

第 3 步——在保存之前，先把摘要作为你自己的消息写出来。这条消息绝不能包含工具调用；写完它就结束本回合。用一行精确的 `## Summary` 开头，在这行标题后面放成稿的摘要 markdown。会议 UI 会实时流式显示这一节——这是运行结束前用户唯一能看到内容的方式——而且它就是你在第 3b 步作为 `<YOUR_SUMMARY>` 传入的同一份 markdown。

这一步不是可选的，也不是收尾报告。「会议 112 已总结并保存到记录中」是一句报告；它不能满足这一步。保存一份你从没打印过的摘要，意味着用户一整个运行期间都盯着占位符，最后什么也没得读——所以把这种情况当成运行失败。规划、工具叙述和保存确认都不要写进标题后面的那一节——那些属于你在第 4 步的收尾消息。

第 3b 步——现在通过专用的 summary 端点保存。服务器会把 `## Summary` 一节合并进笔记本身（用户笔记保留，重跑会替换旧的一节），空摘要会被 400 拒绝，所以丢失的负载会响亮地失败，而不是「成功」为无效操作。先把摘要写进文件——不要用 shell 变量进子进程，不要手拼 JSON：

  cat > /tmp/summary.md <<'SUMMARY_EOF'
  <YOUR_SUMMARY>
  SUMMARY_EOF
  cat > /tmp/title.txt <<'TITLE_EOF'
  <NEW_TITLE_OR_EMPTY>
  TITLE_EOF
  bun -e 'const summary=await Bun.file("/tmp/summary.md").text(),title=(await Bun.file("/tmp/title.txt").text()).trim(),body=title?{summary,title}:{summary};await Bun.write("/tmp/summary.json",JSON.stringify(body))'
  curl -sf -X POST "http://localhost:3030/meetings/<MEETING_ID>/summary" \
    -H "Authorization: Bearer $SCREENPIPE_LOCAL_API_KEY" \
    -H "Content-Type: application/json" \
    --data @/tmp/summary.json

`-f` 很重要：如果这个调用失败，在收尾消息里说明，而不是报告成功。关于标题：如果当前标题缺失、是泛化的（「untitled」「meeting」或只是应用名）或没能反映实际发生的事，就传一个 5-8 词的纯英文标题（不带引号、不用「meeting about…」前缀）——否则传空字符串，让用户设置的标题保持原样。如果没什么值得总结的（转录为空、音频无关），就明说并跳过保存——不要写占位符。

第 4 步——提议把摘要推送到用户已连接的某个应用（先问，绝不自己推）。列出实际已连接的应用，然后让他们一键选择：

  bun -e 'const d=await Bun.file("/tmp/conn.json").json();for(const c of d.data??[])if(c.connected===true)console.log(`${c.id}\t${c.name}`)'   # already fetched in step 1

按相关性给已连接的目标排序——会议期间用过的应用优先（Notion、Slack、Linear……）。然后发一条桌面通知，动作按钮就是这些目标，这样这个请求在 UI 里就渲染成了按钮：

  curl -s -X POST "http://localhost:11435/notify" \
    -H "Content-Type: application/json" \
    -d '{"title": "<TITLE> summarized", "body": "<one-line recap> — push it somewhere?", "priority": "high", "actions": [
          {"label": "推送到 Notion", "type": "api", "method": "POST", "url": "http://localhost:3030/connections/notion/proxy/v1/pages", "body": { /* 由摘要构建的页面负载 */ }},
          {"label": "在聊天中查看", "type": "chat", "prompt": "Review the existing summary for meeting <ID>. Do not rerun meeting-summary.", "context": {"meeting_id": <ID>}},
          {"label": "忽略", "type": "dismiss"}
        ]}'

每个按钮都映射到其 `/connections` `description` 里的一个连接端点（slack/telegram/discord 用 `POST /connections/<id>/send`，notion/linear 等用 `POST /connections/<id>/proxy/...`）。只有当端点、负载和目标都完整时才用 `type: "api"`。当目标需要一个你无法推断的目的地（如 Notion 父页面、Slack 频道）时，用一个指定动作的 `type: "chat"` 按钮，其 prompt 指名现有的会议摘要，并在写入前向用户询问缺失的目的地。绝不要在摘要后按钮上使用 `type: "pipe"` 配合 `pipe: "meeting-summary"` ——那会再次运行总结器。仅查看的按钮也必须用 `type: "chat"`，并告诉聊天只查看已保存的摘要、不要重跑。如果什么都没连接，跳过通知，只需说明连接一个应用就能让你下次推送摘要。
