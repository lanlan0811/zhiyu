# 知屿 Zhīyǔ

纯本地、双模式（日常 / 编码）的 Harness 桌面应用 —— 一个为本地 AI agent 提供工作台的控制平面。

**MIT 许可证** · 完全本地运行 · 无遥测 · 无云端依赖（除用户自行配置的模型 API 外无任何出站请求）

> 本项目由 Waku/CheapRouter（GPL-3.0）逆向分析后**全部重写**而来，仅作功能参考，未沿用任何 GPL 代码。
> 上游参考代码保留在 [`reversed-src/`](reversed-src/) 与 [`reports/`](reports/) 中。

## 特性

- **纯本地**：数据目录 `~/.zhiyu/`，API-Key 本地加密存储（Windows DPAPI / Keyring），不落明文
- **双模式**：
  - **日常模式**：AI 写作（长文 / 改写 / 摘要 / 翻译 / 大纲）、知识库（全文 + 向量混合检索）、问答、内置浏览器（网页研究 → 一键存入知识库）
  - **编码模式**：项目目录工作区、代码读写、终端、Git checkpoint / 回滚、diff 审查、日志与报错分析、Web 调试
- **API 驱动**：OpenAI 双接口协议 —— `chat/completions` + `responses`，SSE 流式
- **内置模型目录**：DeepSeek（`deepseek-v4-pro` / `deepseek-v4-flash`）与 GLM（`glm-5.2` / `glm-5.3` / `glm-5.3-flash`）5 个模型预设，可覆盖、可自定义
- **三大增强功能**：
  - **模型思考强度**：off ~ max 六档 + 路径式 set/unset 补丁（`reasoning_effort` / `reasoning.effort`）
  - **上下文管理系统**：窗口解析（`[1m]` → 1M）、用量追踪（7 来源 breakdown）、摘要式压缩（手动 / 自动 85% → 60%）、模型切换守卫
  - **内置浏览器**：WebView2 内嵌、用户 + Agent 双控（Playwright 风格 locator + CDP）、`browser_execute` 工具、内置 skills

## 架构

```
zhiyu/
├── crates/
│   ├── protocol/     # 类型契约（消息 / 事件 / 命令 / 模式 / 游标 / 思考档位 / 上下文 / 模型配置）
│   ├── driver/       # OpenAI 双接口驱动（chat + responses，SSE 流式）+ 思考档位补丁
│   ├── context/      # 上下文管理（窗口解析 / 用量追踪 / 压缩 / 切换守卫）
│   ├── core/         # 核心引擎（会话 / 持久化 / 工作区 / Git / 技能库 / 知识库）
│   ├── browser/      # 内置浏览器控制（WebView2 / CDP / locator）
│   └── daemon/       # 本地 WebSocket / JSON-RPC 服务（token 认证 + 事件断线重放）
├── src-tauri/        # Tauri v2 桌面壳（窗口 / 托盘 / daemon 生命周期 / token 注入）
└── web/              # React + TypeScript 前端（双模式 UI）
```

## 开发

### 环境要求

| 依赖 | 版本 | 说明 |
|---|---|---|
| Node.js | ≥ 20 | 前端构建（本机必需） |
| Rust | stable | 后端构建 —— **本机可不安装**，构建走 GitHub Actions CI |
| WebView2 Runtime | 随 Windows 10/11 提供 | Tauri 运行环境 |

### 本机前端开发

```bash
cd web
npm ci
npm run dev        # Vite dev server
npm run build      # 生产构建（tsc 类型检查 + vite build）
```

### 后端构建与测试

本机不强制安装 Rust 工具链；构建 / 测试 / 打包全部由 GitHub Actions 完成：

- `ci.yml`：每次 push / PR 触发 —— `cargo build` + `cargo test` + `cargo clippy -D warnings` + 前端构建
- `release.yml`：打 `v*` tag 触发 —— Windows 打包（.msi / .exe）挂到 GitHub Release（draft）

### 里程碑（M1 ~ M8）

开发按里程碑推进，见 [`.zcode/plans/local-harness-rewrite.md`](.zcode/plans/local-harness-rewrite.md)。

- M1 项目骨架 + CI/CD 跑通
- M2 协议层 + 本地 daemon
- M3 OpenAI 双接口驱动 + 内置模型目录 + API-Key 管理
- M4 核心引擎（会话 / 持久化 / Git / 技能库）
- M5 知识库（日常模式）
- M6 三大增强功能（思考强度 / 上下文管理 / 内置浏览器）
- M7 Tauri 壳 + Web 前端（双模式 UI）
- M8 集成验收 + 合规

## 合规

- **许可证**：MIT（见 [LICENSE](LICENSE)）
- **零云端残留**：无遥测、无自动更新、无 `.env.example`；除用户配置的模型 API 外无任何出站请求
- **上游归属**：Waku / CheapRouter（GPL-3.0）为本项目的功能参考来源，见 [NOTICE](NOTICE.md)
