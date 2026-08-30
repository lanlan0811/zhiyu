---
name: web-gui-tester
description: 对 Web 前端做纯 GUI 黑盒测试：场景评估、测试计划、action→observation 循环、截图证据与测试报告。
---

# Web GUI 黑盒测试（web-gui-tester）

通过内置浏览器对目标 Web 应用做黑盒测试，只模拟真实用户操作（点击/输入/滚动），用截图与 DOM 快照验证结果。

## 场景评估

1. 明确被测页面与用户故事（登录、注册、搜索、下单…）。
2. 列出关键路径与边界场景（空输入、错误输入、超长输入、并发）。
3. 标注每个场景的预期结果（可断言）。

## 测试计划

输出结构：

```
## 测试计划
1. 场景：<用户故事>
   - 前置：<页面 URL / 状态>
   - 步骤：
     1) navigate <url>
     2) click <ref>
     3) fill <ref> <value>
   - 预期：<可断言结果>
```

## action → observation 循环

- 每个动作后**必须观测**：`snapshot` 或 `evaluate` 或 `screenshot`。
- 断言失败立即记录，继续下一场景，不阻塞。
- 每次操作前确认元素 `enabled`（快照里为 false 的元素不可操作）。

## 截图证据

- 每个场景至少两张：动作前 / 动作后。
- 测试报告附截图路径与关键快照片段。

## 测试报告

```
## 测试报告
| 场景 | 结果 | 证据 |
|---|---|---|
| 登录成功 | PASS | screenshot_login_ok.png |
| 密码错误提示 | PASS | screenshot_err.png |
| 空提交 | FAIL | screenshot_empty.png（无校验提示） |
```

## 安全规则

- 不输入真实凭据（用测试账号/占位符）。
- 不触发破坏性操作（删除/清库）除非显式要求。
- 测试完不遗留脏数据。
