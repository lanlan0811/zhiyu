# NOTICE

知屿 Zhīyǔ (MIT License) is a clean-room rewrite of the [Waku](https://github.com/egoist/waku) /
CheapRouter desktop client (GPL-3.0-only), rebuilt for pure-local, API-driven operation.

This project is released under the **MIT License**. Per the requirement of the
upstream GPL-3.0 license, the attribution and modification history of the
original code are recorded here for transparency:

- Upstream project: Waku by egoist (GPL-3.0-only), forked and modified as
  CheapRouter with cloud account integration.
- The upstream source (GPL-3.0) was **reverse-analyzed for feature reference
  only** and is preserved in the `reversed-src/` and `reports/` directories.
- **No GPL source code is copied into this project.** All implementation in
  `crates/`, `src-tauri/` and `web/` is newly written under the MIT License.
- The GPUI framework used by the upstream (GPL-3.0-or-later) is replaced with
  Tauri v2 (MIT/Apache-2.0) + React (MIT).

## Reverse-engineering reports

- [`reports/逆向分析报告-Waku-CheapRouter.md`](reports/逆向分析报告-Waku-CheapRouter.md)

## Third-party notices

This project depends on crates and packages that carry their own licenses
(MIT / Apache-2.0 / BSD-3-Clause, etc.); see `Cargo.lock` and `web/package.json`
for the full list.
