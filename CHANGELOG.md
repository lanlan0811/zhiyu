# 更新日志

所有显著变更记录于此（[Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 风格）。

## [Unreleased]

### 新增（M1 ~ M8）

- **M1 项目骨架**：Cargo workspace（protocol/driver/context/core/browser/daemon）、MIT LICENSE、CI/CD workflow（`ci.yml` / `release.yml`）、图标生成脚本。
- **M2 协议层 + 本地 daemon**：类型契约（模式/游标/checkpoint/思考档位/上下文/模型配置）、本地 WebSocket/JSON-RPC 服务、token 认证、事件序号断线重放。
- **M3 OpenAI 双接口驱动**：`chat/completions` + `responses` SSE 流式解析、工具调用、usage 回传、内置模型目录（DeepSeek/GLM 5 模型）、Windows DPAPI API-Key 加密存储 + 多 key 轮换。
- **M4 核心引擎**：双模式会话管理、SQLite/WAL 持久化、turn 级 Git checkpoint/回滚、技能库（SKILL.md 发现）、工作区文件读写。
- **M5 知识库（日常模式）**：md/txt/代码 + PDF/Word 摄取、标题/长度分块、FTS5 全文 + 向量混合检索（RRF 融合）、agent 工具（search/read/save）。
- **M6 三大增强功能**：模型思考强度（六档 + 路径式 set/unset 补丁）、上下文管理（窗口解析/用量追踪/摘要式压缩/切换守卫）、内置浏览器引擎（Tab 管理 + CDP/locator + 内置 skills）。
- **M7 桌面壳 + 双模式 UI**：Tauri v2 壳（daemon 生命周期/token 注入）、React 前端（日常/编码切换、会话、转录流、composer、知识库、写作、工作区、终端、浏览器、模型设置）。
- **M8 集成与合规**：全量 Rust 代码审查修复、零云端残留确认、MIT 许可证、开源文档。

### 修复

- 编译错误：孤儿 impl（E0116）、重复函数（E0428）、类型不匹配（E0308）。
- clippy `-D warnings`：无用导入、`map_unwrap_or`、`useless_vec`。
- 测试：SSE CRLF 帧偏移、Off 档 reasoning_effort 语义、mock server 阻塞读。
- Cargo：`[workspace.dev-dependencies]` 不支持 → `tempfile` 移入 `[workspace.dependencies]`。
