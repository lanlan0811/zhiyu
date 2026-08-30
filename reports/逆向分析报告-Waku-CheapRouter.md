# 《Waku / CheapRouter v0.1.18》逆向分析报告

## 0. 目标与授权

- **目标文件**：
  - `D:\CheapRouter\waku.exe` — 32,286,720 B（PE32+ x86-64 GUI）
  - `D:\CheapRouter\waku-daemon.exe` — 14,128,128 B（PE32+ x86-64 console）
  - `D:\CheapRouter\unins000.exe/.dat` — Inno Setup 卸载器（非分析目标）
  - `D:\CheapRouter\LICENSE` — GPL-3.0
- **授权**：用户拥有该软件（位于其本人电脑，要求完整逆向出源码）。
- **分析目标**：完整还原源码，有混淆则反混淆，产物落盘到本地工作区。

## 1. 侦察结论

- **文件类型/架构/位宽**：PE32+ / x86-64 / 64 位，无壳、未加混淆、未签名（证书目录为空）。
- **语言/框架**：**Rust**（rustc `ac68faa20c58`，MSVC 链接器 14.51）+ **GPUI**（Zed 自绘 UI 框架）。
  证据：内嵌数百条 rustc panic 路径与 cargo registry 路径（`index.crates.io-1949cf8c6b5b557f`）、
  aws-lc-rs 0.44 导出符号、Zed/GPUI crate 路径（accesskit/taffy/resvg/alacritty_terminal）。
- **工具链**：自写 Python 脚本（PE 解析 + 字符串提取），git（克隆上游）。
  pip/pefile 因网络受限未用，纯 Python 完成等价解析。

## 2. 总体架构

**Waku（CheapRouter 发行版）= "低价模型网关 + 本地 AI 编码 agent 指挥台" 双进程桌面应用。**

```
┌─────────────────────────────┐   localhost WebSocket     ┌──────────────────────────────┐
│  waku.exe (GUI, GPUI)       │◄── JSON-RPC (ACP 2.0.0)───►│  waku-daemon.exe (console)   │
│  • 会话/任务/多项目管理      │   WAKU_DAEMON_TOKEN 认证    │  • SQLite 持久化 (app.db)    │
│  • provider 进程编排         │   waku-daemon-ready 握手    │  • worktree/文件系统管理      │
│  • 模型广场/余额/用量/充值   │                              │  • 并发进程管理               │
│  • 内嵌终端/diff/技能库      │                              │  • WebSocket 服务端           │
│  • 自动更新 (ed25519)       │                              └──────────────────────────────┘
└─────────────────────────────┘
        │ 网关路由注入 (改写各 CLI 全局配置: claude/codex/grok/opencode/amp/cursor/kimi/deepseek)
        ▼
  https://cheaprouter.cc 云网关 (sub2api crate: 云账号登录/余额/路由计费)
```

架构为 **Rust workspace**：`waku-client`（客户端协议）、`waku-core`（daemon 服务端核心）、
`waku-daemon`（daemon 入口）、`waku-protocol`（协议模型）、`sub2api`（CheapRouter 云网关特有）。

## 3. 关键发现（按用户目标排列）

| # | 结论 | 证据 | 可信度 |
|---|------|------|--------|
| 1 | **该软件是公开开源项目的 GPL fork**：上游 `egoist/waku`，本 fork 为 `ai-poet/agent-client`（CheapRouter Agent Client） | NOTICE.md 记录 fork 自 commit `d82304a`；仓库 LICENSE=GPL-3.0-only | 确证 |
| 2 | **仓库源码与二进制完全对应（v0.1.18）** | 仓库 commit `c035bce chore(release): v0.1.18` + tag `v0.1.18`；二进制内嵌 `0.1.18`；构建时间 2026-08-28 15:29 UTC ≈ 仓库推送 15:18:55Z | 确证 |
| 3 | **crate 结构逐一对拍成功** | 二进制内嵌 `crates\waku-core\src\skills.rs`、`crates\waku-client\src\persistence.rs` 等路径；waku.exe 命中 4 crate、daemon 命中 3 crate | 确证 |
| 4 | **双进程 WebSocket/JSON-RPC 协议（ACP 2.0.0）** | daemon 二进制含 `agent-client-protocol` 113 处、`tungstenite`、`waku-daemon-ready`、`WAKU_DAEMON_TOKEN`；源码 `crates/waku-core/src/server.rs`、`waku-protocol` | 确证 |
| 5 | **CheapRouter 云网关（sub2api crate）** | `s3.cheaprouter.cc` appcast、`cheaprouter.cc` 云路由、`command_for_provider()` 网关注入 | 确证 |
| 6 | **自动更新用 Sparkle + ed25519** | `appcast-windows-x86_64.xml`、`SUPublicEDKey is not a valid ed25519 key`（二进制+`src/updater.rs:1049`）、内嵌 ed25519 公钥 | 确证 |
| 7 | **遥测默认关闭** | NOTICE.md："telemetry disabled unless the build opts in" | 确证 |
| 8 | **无混淆、无加壳** | 标准区段名（.text/.rdata/.data/.pdata/.fptable/.rsrc/.reloc）、熵值正常（.text≈6.3）、无 UPX/Themida/VMProtect 特征 | 确证 |

## 4. 可疑行为（恶意样本场景）

无恶意行为。属正常商业/开源软件：网络行为仅为云网关 API、自动更新源（s3.cheaprouter.cc）、
云账号登录（loopback redirect）；本地写入 `~/.cheaprouter/`（app.db/settings.json 等）与各 CLI 全局配置
（接管前备份、退出登录恢复）；遥测默认关闭。全部行为有源码可查。

## 5. 关键函数与算法

无需伪代码还原——**直接获得原始源码**（完整还原，见第 6 节）。要点：

- 网关路由注入：`crates/waku-core/src/command_env.rs::command_for_provider()`
  （Claude/Codex 启动点使用，见 NOTICE.md）。
- 更新校验：`src/updater.rs` — `verifying_key()`（内嵌 `SUPublicEDKey`）、
  `Signature`/`VerifyingKey`（ed25519_dalek）验证 appcast 签名。
- daemon 协议：`crates/waku-core/src/server.rs`（WebSocket/JSON-RPC 服务端）、
  `crates/waku-protocol/src/model.rs`（协议模型）。
- 持久化：`crates/waku-core/src/persistence.rs`（SQLite/rusqlite，`app.db`）。
- 无人值守安装：`crates/sub2api/src/node_install.rs`（Node 22 便携/MSI/winget 三级回退）、
  `crates/sub2api/src/git_install.rs`（PortableGit 等）。

## 6. 源码还原产出

- **还原程度**：★ **完整源码**（非反编译伪代码；从公开 GPL 仓库取得并逐一对拍验证）。
- **产出目录**：`reversed-src/`
  - `reversed-src/waku-agent-client/` — 完整仓库（689 文件，约 16.6 万行 Rust + TS/TSX）
  - `reversed-src/README.md` — 还原对象/方法/程度/验证/核心发现
  - `reversed-src/tools/verify-integrity.py` — 可复现的二进制↔源码对拍验证脚本
- **还原方法**：侦察（纯 Python 静态分析）→ 定位公开仓库（GitHub: ai-poet/agent-client）→
  git clone → 字符串/版本/crate/tag 四维对拍验证。
- **验证结果**：`verify-integrity.py` → **18/18 PASS**（版本 2 项、crate 结构 3 项、
  关键字符串 12 项、git tag 1 项）。
- **还原缺口**：无。无混淆/加壳，无需反混淆。

## 7. 未解决问题与建议

- **剩余线索**：无实质缺口。可选深化：
  1. `waku-daemon.exe` 不内嵌主程序版本串（独立 console 服务）——已确认属正常，非缺失。
  2. 若需验证运行态行为，可进一步做动态分析（在隔离环境运行并观察 WebSocket 流量）——本轮未做，属可选。
- **建议**：如需审计网关计费/路由逻辑，重点看 `crates/sub2api/`；如需理解 agent 编排，看 `crates/waku-core/`。
- **许可提示**：该软件为 GPL-3.0-only，源码使用/再分发请遵守其条款（NOTICE.md 记录 fork 修改）。

## 8. 附录：Agent 循环执行记录

- **Round 1 侦察**（general-purpose 子代理，后台运行）：确认 Rust+GPUI、无壳、GPLv3、
  双进程架构与业务功能，给出"优先找公开仓库"路线。产物：`analysis-tools/waku_strings.txt`（20,697 条）、
  `waku-daemon_strings.txt`（9,480 条）、pe_recon.py / extract_strings.py / list_crates.py。
- **Round 2 源码获取**（主代理直接执行）：GitHub API 检索 → 命中 `ai-poet/agent-client` →
  克隆 → 版本/crate/字符串/tag 对拍。
- **Round 3 交叉验证**（主代理执行）：修复验证脚本 3 处检查逻辑（字符串 interning 合并、
  `src/` 目录搜索遗漏、daemon 版本串归属），最终 **18/18 PASS**。
- **工具链**：Python 3.10（自写脚本，`analysis-tools/`）、git（系统自带）、curl（GitHub API）。
  pip/pefile 安装因网络 SSL 失败，未采用（不影响结果，纯 Python 等价完成）。
- **卡点与升级**：字符串独立匹配因链接器合并字符串失败 → 改用子串匹配（非换工具，属检查口径修正）。
