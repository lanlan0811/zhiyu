# 贡献指南

感谢你愿意为 **知屿 Zhīyǔ** 贡献代码！请阅读以下约定。

## 项目约定

- **许可证**：MIT。所有贡献默认以 MIT 授权。
- **纯本地**：本项目无遥测、无自动更新、无云端依赖。除用户配置的模型 API 外无任何出站请求。新增依赖时请保持这一点。
- **双系统兼容**：代码需在 Windows 与 macOS 上均可构建运行（CI 覆盖 Windows）。
- **无硬编码**：模型地址、端口、路径等配置项一律走设置/常量/环境注入。
- **图标**：不使用 emoji 作为图标，统一使用 SVG/位图资源。

## 工作流

1. `git checkout -b feature/xxx` 新建分支。
2. 开发 + 提交，提交信息遵循 [docs/commit-messages.md](docs/commit-messages.md) 的风格（未迁移，参考简洁 subject + 正文）。
3. 推送分支，开 Pull Request 到 `main`。
4. PR 触发 CI（`.github/workflows/ci.yml`）：`cargo build` + `cargo test` + `cargo clippy -D warnings` + 前端 `npm run build`。全绿才可合并。

## 本地开发

- **前端**：`cd web && npm ci && npm run dev`（Node ≥ 20）。
- **后端**：本机可不装 Rust；构建/测试依赖 CI。如需本地编译，安装 Rust stable 后 `cargo build --workspace && cargo test --workspace`。

## 代码结构

| 目录 | 职责 |
|---|---|
| `crates/protocol` | 类型契约（消息/事件/命令/模式/思考档位/上下文/模型配置） |
| `crates/driver` | OpenAI 双接口驱动（chat/responses，SSE 流式）+ 思考档位补丁 |
| `crates/context` | 上下文管理（窗口解析/用量追踪/压缩/切换守卫） |
| `crates/core` | 核心引擎（会话/持久化/工作区/Git/技能库/知识库） |
| `crates/browser` | 内置浏览器控制（WebView2/CDP/locator） |
| `crates/daemon` | 本地 WebSocket/JSON-RPC 服务（token 认证 + 事件重放） |
| `src-tauri` | Tauri v2 桌面壳 |
| `web` | React + TS 前端 |

## 测试

每个 crate 都带单元测试；核心逻辑（协议编解码、SSE 解析、思考补丁、上下文、模型目录、keyring、知识库检索、Git checkpoint）要求有覆盖。新增功能请同步补测试。
