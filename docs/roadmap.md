# 知迹（Screenpipe）路线图

<!-- doc-covers: none -->

## 已完成基线

- README 与项目文档已切换为本地优先的个人知识库定位。
- 历史方案、旧 Mintlify 迁移资产和团队/云端方向文档已清理。

## 进行中

- **Local Brain 首版基础改造与修复**（updated_at: 2026-09-08）：[PRD](prd/personal-brain-local-first.md)、[TRD](trd/personal-brain-local-first.md)已纳入本次确认的入口分工和统一任务基础能力。飞书/腾讯会议仍为首版两个只读办公入口，WPS与国内Runtime新适配后置；复用选中Preset并允许手动切换已有Runtime，保留无费用/每日调用硬上限。
  - **入口分工**：知识库呈现工作单元/知识/画布；聊天承担提问、引用和纠错；系统活动按业务运行聚合消息与状态；自动化管理内置/用户任务定义；连接维护办公授权、账号、范围和同步配置。工作单元以实际活动为锚，关联办公资料补证，知识仍需自审发布。
  - **基础改造**：复用Pipes定义/触发和Brain持久队列，统一run/attempt/event、取消/重试/恢复与资源接纳；活动总结、Brain、办公同步及用户Pipes逐类型迁移到一个调度所有者。保留业务校验、账号游标、用户Pipes权限/输出及历史，连接配置不出现第二套编辑真源。
  - 本次文档修订核对代码HEAD `0a83343ac`，修订前工作区干净。[原U计划](plans/personal-brain-local-first-execution-plan.md)与[历史验收记录](reviews/personal-brain-local-first-acceptance.md)保留溯源；已有模块不等于完整闭环已通过。
  - [原实施审查](reviews/personal-brain-implementation-readiness.yaml)针对`5fcacf6b1`的37项，结果blocked：删除/恢复、历史、抽取、版本、检索引用、办公控制、Runtime和审核存在缺口。10个合成反例复现、1个对照通过；前端两组测试83通过/1失败。该报告保持历史快照，不宣称已覆盖当前HEAD或新增范围，正式真实样本与持续使用仍待验收。
  - [本次基础改造与修复计划](plans/personal-brain-correctness-execution-plan.md)已更新（approved，**F01基线已验收，B01整批实施中**）：保留F01–F11，增加F12公共任务、F13 Brain纵向接管、F14活动/办公/Pipes迁移，共14节点串行。按用户最新指示由当前会话直接完成整批代码与自测，不安排子智能体，最后统一审查/验证且不逐节点交回；同一问题三轮修正失败则停止征询。先删除/恢复和公共任务，再业务迁移与五入口，F11汇总原37项＋R12/R13新增8项＝45项。文档结构校验见[计划校验记录](reviews/personal-brain-foundation-plan-validation.json)，不代替产品测试；完成并验证后再并入已完成基线。

- **桌面端前端完全汉化**：以当前 `zh-local` / Local Brain 定位为准，覆盖主导航、首页、设置、聊天、时间线、首次启动、通知、错误和空状态等用户可见路径；保留代码标识、协议字段、模型名和必要的第三方专有名词。品牌历史实现见原U13，本次残留修复和新入口由F10承接，与本项共享文件串行修改。

## 待规划

- **Local Brain 后续生态增强**：WPS 接入、向量融合、外部只读 MemoryProvider、其他办公平台及国内 Runtime 具名 MCP/ACP 兼容（R10）按需分别规划；全账号历史同步、双向办公操作与多 Runtime 调度不计入首版。
- 建立稳定的前端文案资源边界，减少散落硬编码文案。
- 为主要用户路径补充中文界面回归检查和截图验收。
