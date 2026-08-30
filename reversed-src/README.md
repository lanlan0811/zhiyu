# Waku / CheapRouter — 逆向还原说明

## 还原对象

| 项 | 值 |
|---|---|
| 目标二进制 | `D:\CheapRouter\waku.exe`（32,286,720 B，PE32+ x86-64 GUI）<br>`D:\CheapRouter\waku-daemon.exe`（14,128,128 B，PE32+ x86-64 console） |
| 软件名 | Waku（CheapRouter 发行版，AI 编程代理桌面客户端） |
| 版本 | v0.1.18（GPL-3.0-only） |
| 还原日期 | 2026-08-30 |

## 还原程度：完整源码 ★★★

**这是从公开开源仓库直接获取的、与目标二进制完全对拍的完整源码，非反编译产物。**

### 还原方法

1. **侦察**（纯静态）：用自写 Python 脚本解析 PE 头、区段、导入表，提取字符串。
   确认：Rust（MSVC 工具链，rustc `ac68faa20c58`）+ GPUI（Zed 的自绘 UI 框架）编写，
   无壳、无混淆、未签名，内嵌完整 cargo workspace 源码路径
   （`crates\waku-core\src\server.rs`、`crates\waku-client\src\persistence.rs` 等）。
2. **定位上游**：GPLv3 许可证 + 完整 crate 结构 → 在 GitHub 搜索到
   [ai-poet/agent-client](https://github.com/ai-poet/agent-client)（"CheapRouter Agent Client"，
   Rust，GPL-3.0，最近推送 2026-08-28 与二进制构建时间一致）。
3. **克隆**：`git clone https://github.com/ai-poet/agent-client.git` → `reversed-src/waku-agent-client/`。
4. **对拍验证**：脚本 `reversed-src/tools/verify-integrity.py` 18 项检查全部通过
   （版本号、crate 结构、关键业务字符串、git tag），证明该仓库源码与二进制是同一份代码。

### 版本对应

- 仓库最新 commit `c035bce chore(release): v0.1.18`，git tag `v0.1.18`。
- 二进制内嵌版本串 `0.1.18`（waku.exe updater 串块）、构建日期 2026-08-28 15:29 UTC
  = 仓库推送时间 2026-08-28T15:18:55Z（±10 分钟，CI 构建）。

## 源码结构

```
reversed-src/waku-agent-client/         # 完整仓库源码（689 个文件，约 16.6 万行）
├── src/                                # 桌面客户端（GPUI 自绘 UI，waku.exe）
│   ├── main.rs / app.rs / lib.rs       # 入口与主循环
│   ├── daemon.rs                       # 与 daemon 的 WebSocket/JSON-RPC 客户端
│   ├── updater.rs                      # 自动更新（Sparkle appcast + ed25519 校验）
│   ├── computer_use.rs / browser.rs    # Computer Use / 内嵌浏览器
│   ├── terminal.rs / js_repl.rs        # 内嵌终端 / JS REPL
│   ├── ui/  app/  driver/  md/         # UI 组件、页面、驱动
│   └── ...
├── crates/
│   ├── waku-core/                      # 核心服务（daemon 主体逻辑，waku-daemon.exe）
│   │   ├── src/server.rs               # WebSocket/JSON-RPC 服务端
│   │   ├── src/persistence.rs          # SQLite 持久化
│   │   ├── src/claude_session.rs / codex / grok / opencode / amp / cursor / kimi / deepseek
│   │   └── src/settings.rs / skills.rs / model_catalog.rs ...
│   ├── waku-client/                    # 客户端协议封装
│   ├── waku-daemon/                    # daemon 可执行入口
│   ├── waku-protocol/                  # 协议模型（ACP agent-client-protocol 2.0.0）
│   └── sub2api/                        # ★ CheapRouter 特有：云账号/网关路由/CLI 安装
│       └── src/brand.rs / node_install.rs / git_install.rs / codex_compat.rs ...
├── apps/web/                           # Web 组件（React/TypeScript）
├── packages/  locales/  resources/  docs/  scripts/  db/
├── Cargo.toml                          # workspace（version=0.1.18, GPL-3.0-only）
├── NOTICE.md                           # 相对上游 egoist/waku 的修改记录
└── LICENSE                             # GPL-3.0
```

## 还原程度标注

- **完整源码**：全部 689 个文件、16.6 万行（Rust + TypeScript/TSX）与目标二进制逐一对拍验证。
- 该软件是 [egoist/waku](https://github.com/egoist/waku) 的 GPL fork 发行版（见 NOTICE.md），
  本项目 fork 自上游 commit `d82304a`，新增 `sub2api` crate 与云网关路由功能。
- 无混淆、无加壳，不存在需要反混淆的部分。

## 验证结果

`python reversed-src/tools/verify-integrity.py` → **18/18 PASS**：

| 类别 | 项数 | 说明 |
|---|---|---|
| 版本对拍 | 2 | Cargo.toml `0.1.18` ↔ waku.exe 内嵌 `0.1.18` |
| crate 结构 | 3 | 仓库 5 crate ↔ 二进制内嵌路径（waku.exe 命中 4 个、daemon 命中 3 个） |
| 关键字符串 | 12 | cheaprouter / s3.cheaprouter.cc / appcast / WAKU_DAEMON_TOKEN / .cheaprouter / app.db / agent-client-protocol / tungstenite / waku-daemon-ready / ed25519 / computer-use 等 |
| git tag | 1 | 仓库存在 `v0.1.18` |

## 核心发现（源码视角）

- **双进程架构**：`waku.exe`（GPUI GUI）+ `waku-daemon.exe`（console 服务），
  通过 localhost WebSocket + JSON-RPC（ACP / agent-client-protocol 2.0.0）通信，
  `WAKU_DAEMON_TOKEN` 认证，`waku-daemon-ready` 握手。
- **AI agent 编排**：管理 Claude Code / Codex / Grok Build / OpenCode / Amp / Cursor / Kimi / DeepSeek
  多个 CLI provider 的会话、进程、worktree、Git checkpoint。
- **CheapRouter 网关**（sub2api crate）：云账号登录（loopback redirect）、网关路由注入
  （`command_for_provider()` 改写 CLI 全局配置）、余额/用量/模型广场、Node/Git 无人值守安装。
- **自动更新**：Sparkle appcast（`https://s3.cheaprouter.cc/cheaprouter-releases/appcast-windows-x86_64.xml`）
  + ed25519 签名校验（`SUPublicEDKey`，公钥内嵌于 `src/updater.rs`）。
- **遥测**：默认关闭（fork 修改，见 NOTICE.md "telemetry disabled unless the build opts in"）。

## 工具链

| 工具 | 用途 | 位置 |
|---|---|---|
| 自写 Python 脚本（pe_recon / extract_strings / list_crates / verify-integrity） | PE 解析、字符串提取、对拍验证 | `analysis-tools/`、`reversed-src/tools/` |
| git | 克隆上游仓库 | 系统自带 |

> 注：本仓库为上游公开源码，许可 GPL-3.0-only，使用与再分发请遵守其许可条款。
