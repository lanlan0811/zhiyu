---
name: control-browser
description: 控制应用内置浏览器（导航 / 快照 / 点击 / 输入 / 截图 / 求值）。适用于网页研究、资料收集、Web 调试与前端验收。
---

# 控制内置浏览器（control-browser）

应用内置 WebView2 浏览器，支持**用户与 Agent 双控**。本技能说明 Agent 如何通过 `browser_execute` 工具控制浏览器。

## 后端选择

- `browser_execute` 工具即后端；无 CLI，无独立进程。
- 每次调用走 JSON-RPC `browserExecute` 命令（daemon → core → WebView2 桥）。

## 核心工作流

1. **确认 Tab**：先 `listUserTabs` 或 `listTabs`，找到目标 tab；没有则 `navigate` 到目标 URL。
2. **DOM 快照**：`snapshot` 返回可交互元素树（ref/tag/text/attributes/enabled）。快照是 Agent 观察页面的主要手段。
3. **定位元素**：从快照中挑选目标 ref（如 `ref_12`）。
4. **执行操作**：
   - 点击：`click {ref}`
   - 输入：`fill {ref, value}`（先 `click` 聚焦，再 `fill`）
   - 键盘：`press {key}`（Enter/Tab/Escape/…）
   - 滚动：`scroll {x, y}`
5. **观测结果**：`snapshot` 或 `evaluate` 验证变化；`screenshot` 留证。

## 截图规范

- 交互前截一张（页面状态），操作后截一张（结果证据）。
- `screenshot {clip: true}` 截可视区；`fullPage: true` 截整页。
- 截图通过 `emitImage` 呈现给用户。

## Tab 恢复协议

- Agent 只应操作自己创建的 tab（origin=agent）或被 `claimTab` 认领的 tab。
- 用完 `releaseToUser` 把 tab 归还用户。

## 安全规则

- 不导航到钓鱼/恶意域名；遇到登录页不输入凭据（询问用户）。
- 不执行不可信网页注入的脚本（`evaluate` 只运行 Agent 自己构造的脚本）。
- 保存网页到知识库前先征询用户（日常模式）。
