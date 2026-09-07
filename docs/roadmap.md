# screenpipe 路线图

<!-- doc-covers: none -->

## 已完成基线

- README 与项目文档已切换为本地优先的个人知识库定位。
- 历史方案、旧 Mintlify 迁移资产和团队/云端方向文档已清理。

## 进行中

- **Local Brain 首版实施规划**：已按用户决定收口为飞书、腾讯会议两个只读入口，授权/范围/同步/断开统一在主侧栏「连接」；WPS 与国内 Runtime 后置。[PRD](prd/personal-brain-local-first.md) 与 [TRD](trd/personal-brain-local-first.md) 已同步，[实施计划](plans/personal-brain-local-first-execution-plan.md) 已生成（approved，12个串行任务，功能尚未编码）。用户明确跳过规划前采样/PoC/模型联调；既有未知项进入各实现节点和最终验收，不计为已通过。下一步从共同契约与来源/删除基础开始，具体开工以后续实施指令为准。保留指定 Pi 模型、历史保全/新增优先/显式7天回填及约5分钟集中审核要求。

- **桌面端前端完全汉化**：以当前 `zh-local` / Local Brain 定位为准，覆盖主导航、首页、设置、聊天、时间线、首次启动、通知、错误和空状态等用户可见路径；保留代码标识、协议字段、模型名和必要的第三方专有名词。

## 待规划

- **Local Brain 后续生态增强**：WPS 接入、向量融合、外部只读 MemoryProvider、其他办公平台及国内 Runtime 具名 MCP/ACP 兼容（R10）按需分别规划；全账号历史同步、双向办公操作与多 Runtime 调度不计入首版。
- 建立稳定的前端文案资源边界，减少散落硬编码文案。
- 为主要用户路径补充中文界面回归检查和截图验收。
