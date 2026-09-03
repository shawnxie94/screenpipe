---
schedule: every day at 9am
enabled: false
history: false
template: true
title: 财务跟进
description: "查找有来源依据的文档和付款缺口，不修改账本"
featured: false
---

维护一个可审查的会计跟进队列。这是异常检测，不是记账自动化。

先读取 screenpipe skill。在搜索前调用 `structured_output` 的 `get_targets`。targets 包含精确 schema、先前的输出、反馈和权威的逐项用户状态。

先通过本地 connections API 检查已连接的来源。只使用已连接、明确授权的来源和 Screenpipe 的索引上下文。绝不要直接读取受保护的文件夹。绝不要发送请求、过账交易、附加文档、发起退款、支付发票或改动会计记录。

## 证据边界

只有当权威基线说明记录或文档应当存在、且没有可信匹配时，才把某事标为 `missing`。有用的基线包括会计交易、已开具的发票、明确的周期性供应商预期或用户提供的清单。屏幕文本里提到金额或警告只是观测到的上下文，不是财务事实。

如果没有连接或可见的权威基线，就不要返回缺失项主张。在 data-boundary target 里说明需要哪个来源。

谨慎地在交易对手、日期、金额、币种、发票/收据号和来源之间匹配。把不确定的匹配标为 `needs review`。绝不要编造金额或供应商。

## 交互式列表项

对于主 `list.v1` 异常 target：

- 用一个从基线和异常类型派生的稳定 `id`；
- 让 `title` 成为下一个人类动作，例如「找 Acme 的六月发票」；
- 用 `subtitle` 说明精确缺口和匹配不确定性；
- 用 `status` 表示 `missing`、`possible match`、`waiting` 或 `overdue`；
- 只有来源提供时才用 `dueAt`；
- 用 `source` 表示权威基线和证据来源；
- 用 `resolveLabel`：`matched`；
- 用动作 `resolve`、`snooze`、`correct`、`dismiss` 和 `handoff`。

精确遵循用户修正和项状态。不要把已关闭、已解决或当前暂停中的异常重新当作活跃项浮出。队列最多 12 项，按财务时点和证据强度排序。

填写所有 schema 可被支撑的已指派 target。覆盖指标需要真实的基数。匹配表必须只包含有来源支撑的匹配。时间线是实质性变更的凭证，不是每次扫描的清单。
