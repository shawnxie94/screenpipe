# 知迹（Screenpipe）路线图

<!-- doc-covers: none -->

## 已完成基线

- README 与项目文档已切换为本地优先的个人知识库定位。
- 历史方案、旧 Mintlify 迁移资产和团队/云端方向文档已清理。

## 进行中

- **Local Brain 首版实施**：已按用户决定收口为飞书、腾讯会议两个只读入口，授权/范围/同步/断开统一在主侧栏「连接」；WPS 与国内 Runtime 后置。[PRD](prd/personal-brain-local-first.md) 与 [TRD](trd/personal-brain-local-first.md) 已同步，[实施计划](plans/personal-brain-local-first-execution-plan.md)（approved，13个串行任务、37项首版验收）。2026-09-07 U01–U11、U13 代码全部落地且各层测试绿（db 163 / connect office 18 / engine brain 15 / MCP 92 / 前端品牌 30），桌面开发版构建启动，对运行实例完成 REST 全量走查并修复 5 个 bug（记录见[验收记录](reviews/personal-brain-local-first-acceptance.md)，含实过/残留/未测三分）；品牌替换按[具名保留清单](reviews/personal-brain-brand-retained-identifiers.md)完成。正式验收待外部授权件解除具名残留：① lark-cli 补 `im` 读 scope；② 安装 tmeet；③ 真实模型回答验证（2026-09-08 起绑定已改为复用「模型与密钥」选中预设+手动切换 Runtime，不再固定预设）。解除后重跑 U12 真实样本验收再并入已完成基线。未创建 git 提交（工作树含 462 个他人未提交汉化文件，须分批严格分离）。

- **桌面端前端完全汉化**：以当前 `zh-local` / Local Brain 定位为准，覆盖主导航、首页、设置、聊天、时间线、首次启动、通知、错误和空状态等用户可见路径；保留代码标识、协议字段、模型名和必要的第三方专有名词。产品品牌替换统一由Local Brain计划U13负责，与本项共享文件串行修改。

## 待规划

- **Local Brain 后续生态增强**：WPS 接入、向量融合、外部只读 MemoryProvider、其他办公平台及国内 Runtime 具名 MCP/ACP 兼容（R10）按需分别规划；全账号历史同步、双向办公操作与多 Runtime 调度不计入首版。
- 建立稳定的前端文案资源边界，减少散落硬编码文案。
- 为主要用户路径补充中文界面回归检查和截图验收。
