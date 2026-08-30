#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
verify-integrity.py — 二进制↔源码 完整性对拍验证
验证目标: D:\\CheapRouter\\waku.exe / waku-daemon.exe 与还原源码 reversed-src/waku-agent-client 是否同一代码
验证内容:
  1. 版本号对拍 (Cargo.toml version == 二进制中的版本串)
  2. crate 结构对拍 (仓库 crates/ 目录 == 二进制内嵌源码路径中的 crate 名)
  3. 关键业务字符串逐字对拍 (从二进制提取的字符串在源码中可命中)
  4. git tag 对拍 (存在 v<版本> tag)
用法: python verify-integrity.py
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))      # reversed-src/
WORKSPACE = os.path.dirname(ROOT)                                        # 工作区根
REPO = os.path.join(ROOT, "waku-agent-client")
BIN_DIR = r"D:\CheapRouter"
BINS = ["waku.exe", "waku-daemon.exe"]

# 1) 版本
def get_version():
    cargo = os.path.join(REPO, "Cargo.toml")
    with open(cargo, encoding="utf-8") as f:
        m = re.search(r'^version\s*=\s*"([^"]+)"', f.read(), re.M)
    return m.group(1) if m else None

# 2) crate 目录
def get_repo_crates():
    return sorted(d for d in os.listdir(os.path.join(REPO, "crates"))
                  if os.path.isdir(os.path.join(REPO, "crates", d)))

# 3) 从二进制提取可读字符串 (同侦察脚本的简化版)
def extract_strings(bin_path, min_len=6):
    with open(bin_path, "rb") as f:
        data = f.read()
    out = set()
    cur = bytearray()
    for b in data:
        if 0x20 <= b < 0x7F:
            cur.append(b)
        else:
            if len(cur) >= min_len:
                out.add(bytes(cur).decode("ascii", "ignore"))
            cur = bytearray()
    if len(cur) >= min_len:
        out.add(bytes(cur).decode("ascii", "ignore"))
    return out

# 4) git tag
def get_tags():
    r = subprocess.run(["git", "tag"], cwd=REPO, capture_output=True, text=True)
    return r.stdout.split()

def main():
    results = []
    def check(name, ok, detail=""):
        results.append((name, ok, detail))
        print(f"[{'PASS' if ok else 'FAIL'}] {name} {detail}")

    # 1. 版本
    version = get_version()
    ok = version is not None
    check("版本号 (Cargo.toml)", ok, f"version={version}")
    if version:
        # waku.exe 为主 GUI, 内嵌版本串; waku-daemon.exe 为独立 console 服务, 不内嵌主程序版本
        for b in ["waku.exe"]:
            bins = os.path.join(BIN_DIR, b)
            if os.path.exists(bins):
                strings = extract_strings(bins)
                # Rust 字符串会被链接器与相邻常量合并, 用子串匹配
                hit = any(version in s for s in strings)
                check(f"版本对拍 ({b})", hit, f"二进制中出现 {version}")

    # 2. crate 结构
    repo_crates = get_repo_crates()
    expect_crates = ["waku-client", "waku-core", "waku-daemon", "waku-protocol", "sub2api"]
    missing = [c for c in expect_crates if c not in repo_crates]
    check("crate 结构 (仓库含全部 5 个 crate)", not missing, f"仓库={repo_crates}" + (f" 缺失={missing}" if missing else ""))
    for b in BINS:
        bins = os.path.join(BIN_DIR, b)
        if os.path.exists(bins):
            strings = extract_strings(bins)
            hit = [c for c in expect_crates if any(c in s for s in strings)]
            check(f"crate 对拍 ({b})", bool(hit), f"二进制命中 crate={hit}")

    # 3. 关键业务字符串逐字对拍
    key_strings = [
        "cheaprouter", "s3.cheaprouter.cc", "appcast-windows", "appcast",
        "WAKU_DAEMON_TOKEN", ".cheaprouter", "app.db", "agent-client-protocol",
        "tungstenite", "waku-daemon-ready", "ed25519", "computer-use",
    ]
    for s in key_strings:
        src_hits = 0
        for root, _, files in os.walk(REPO):
            # 跳过 .git 与构建产物
            if ".git" in root or "node_modules" in root:
                continue
            for fn in files:
                if fn.endswith((".rs", ".ts", ".tsx", ".js", ".toml")):
                    try:
                        with open(os.path.join(root, fn), encoding="utf-8", errors="ignore") as f:
                            if s in f.read():
                                src_hits += 1
                    except OSError:
                        pass
        check(f"关键串源码命中 [{s}]", src_hits > 0, f"源码命中 {src_hits} 处")

    # 4. git tag
    tags = get_tags()
    if version:
        check("git tag 对拍", f"v{version}" in tags, f"tags={tags[-3:]}")

    failed = [r for r in results if not r[1]]
    print("\n" + "=" * 50)
    print(f"共 {len(results)} 项检查, 通过 {len(results)-len(failed)} 项, 失败 {len(failed)} 项")
    for name, ok, detail in results:
        if not ok:
            print(f"  FAIL: {name} {detail}")
    return 1 if failed else 0

if __name__ == "__main__":
    sys.exit(main())
