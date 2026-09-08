# 知迹（Screenpipe）路线图

<!-- doc-covers: none -->

## 已完成基线

- README 与项目文档已切换为本地优先的个人知识库定位。
- 历史方案、旧 Mintlify 迁移资产和团队/云端方向文档已清理。

## 进行中

- **Local Brain 首版实施与修复**（updated_at: 2026-09-08）：首版范围为飞书、腾讯会议两个只读入口，操作统一在主侧栏「连接」；WPS 与国内 Runtime 新兼容后置。[PRD](prd/personal-brain-local-first.md)、[TRD](trd/personal-brain-local-first.md) 与[原实施计划](plans/personal-brain-local-first-execution-plan.md)保留为首版依据。按 2026-09-08 用户决定复用「模型与密钥」选中预设并允许手动切换 Runtime，不恢复固定模型限制。
  - 当前代码与汉化已提交至 `5fcacf6b1`，本轮审查开始时工作区干净。原 U01–U11、U13 已有模块实现及[历史验收记录](reviews/personal-brain-local-first-acceptance.md)，但不能据此认定完整闭环已经完成。
  - [实施质量审查](reviews/personal-brain-implementation-readiness.yaml)为 `blocked`：发现删除/恢复、历史迁移、增量抽取、版本事务、检索引用、办公控制与证据链、Runtime、审核界面的实质缺口。10 个合成契约反例复现、1 个正常检索对照通过；基础测试通过，现有两组 Brain 前端测试为 83 通过/1 个汉化断言失败。37 项逐项状态见报告，正式真实样本与持续使用仍未完成。
  - [正确性修复计划](plans/personal-brain-correctness-execution-plan.md)已生成（approved，F01–F11 串行，**尚未开工**）：先修删除/恢复和历史，再修抽取/版本/回答，补齐连接与审核闭环，最后回到原 37 项验收。后续代码修复可先用合成夹具推进，真实账号能力/转写与模型质量在最终验收中单独记录；不将缺少授权件当作全部剩余工作。全部完成后再并入已完成基线。

- **桌面端前端完全汉化**：以当前 `zh-local` / Local Brain 定位为准，覆盖主导航、首页、设置、聊天、时间线、首次启动、通知、错误和空状态等用户可见路径；保留代码标识、协议字段、模型名和必要的第三方专有名词。产品品牌替换统一由Local Brain计划U13负责，与本项共享文件串行修改。

## 待规划

- **Local Brain 后续生态增强**：WPS 接入、向量融合、外部只读 MemoryProvider、其他办公平台及国内 Runtime 具名 MCP/ACP 兼容（R10）按需分别规划；全账号历史同步、双向办公操作与多 Runtime 调度不计入首版。
- 建立稳定的前端文案资源边界，减少散落硬编码文案。
- 为主要用户路径补充中文界面回归检查和截图验收。
