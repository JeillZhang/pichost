#!/usr/bin/env bash
set -euo pipefail
# 用法: bash scripts/tests/repo_publish_test.sh
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# ① 语法
bash -n "$ROOT/scripts/publish-repo.sh"
bash -n "$ROOT/scripts/setup-repo.sh"

# ② setup-repo.sh 分支断言
grep -q 'gpg --dearmor' "$ROOT/scripts/setup-repo.sh" \
  || { echo "FAIL: setup apt key missing"; exit 1; }
grep -q 'sources.list.d/pichost.list' "$ROOT/scripts/setup-repo.sh" \
  || { echo "FAIL: setup apt sources missing"; exit 1; }
grep -q 'rpm --import' "$ROOT/scripts/setup-repo.sh" \
  || { echo "FAIL: setup rpm key missing"; exit 1; }
grep -q 'yum.repos.d/pichost.repo' "$ROOT/scripts/setup-repo.sh" \
  || { echo "FAIL: setup dnf repo missing"; exit 1; }

# ③ publish-repo.sh 布局(需要 dpkg-scanpackages;缺工具则跳过功能部分)
if command -v dpkg-scanpackages >/dev/null 2>&1 && command -v gzip >/dev/null 2>&1; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    # 造一个最小假 deb(dpkg-deb --build)
    FAKE="$TMP/fake"
    mkdir -p "$FAKE/DEBIAN"
    printf 'Package: pichost\nVersion: 0.23.0\nArchitecture: amd64\nMaintainer: t <t@t>\nDescription: fake\n' \
      > "$FAKE/DEBIAN/control"
    mkdir -p "$TMP/debs" "$TMP/rpms"
    dpkg-deb --build "$FAKE" "$TMP/debs/pichost_0.23.0_amd64.deb" >/dev/null
    mkdir -p "$TMP/rpms/x86_64"
    : > "$TMP/rpms/x86_64/pichost-0.23.0-1.x86_64.rpm"   # 假 rpm(布局测试仅需文件)
    GPG_SIGN=0 bash "$ROOT/scripts/publish-repo.sh" "$TMP/repo" "$TMP/debs" "$TMP/rpms" stable
    [ -f "$TMP/repo/apt/dists/stable/main/binary-amd64/Packages.gz" ] \
      || { echo "FAIL: Packages.gz missing"; exit 1; }
    [ -f "$TMP/repo/apt/dists/stable/Release" ] \
      || { echo "FAIL: Release missing"; exit 1; }
    [ -f "$TMP/repo/apt/pool/main/p/pichost/pichost_0.23.0_amd64.deb" ] \
      || { echo "FAIL: pool deb missing"; exit 1; }
    [ -d "$TMP/repo/rpm/x86_64" ] || { echo "FAIL: rpm arch dir missing"; exit 1; }
else
    echo "WARN: dpkg-scanpackages missing; publish layout functional check skipped"
fi
echo "repo_publish_test.sh PASS"
