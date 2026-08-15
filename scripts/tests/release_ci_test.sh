#!/usr/bin/env bash
# scripts/tests/release_ci_test.sh — release.yml 双架构矩阵 + deb/rpm 打包断言
set -euo pipefail
# 用法: bash scripts/tests/release_ci_test.sh
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WF="$ROOT/.github/workflows/release.yml"

grep -q 'x86_64-unknown-linux-gnu' "$WF" || { echo "FAIL: amd64 target missing"; exit 1; }
grep -q 'aarch64-unknown-linux-gnu' "$WF" || { echo "FAIL: arm64 target missing"; exit 1; }
grep -q 'zigbuild' "$WF" || { echo "FAIL: zigbuild missing"; exit 1; }
grep -q 'cargo deb' "$WF" || { echo "FAIL: cargo deb missing"; exit 1; }
grep -q 'generate-rpm' "$WF" || { echo "FAIL: generate-rpm missing"; exit 1; }
grep -q 'cargo clippy --workspace -- -D warnings' "$WF" \
  || { echo "FAIL: clippy gate lost"; exit 1; }
grep -q 'cargo test --workspace' "$WF" || { echo "FAIL: test gate lost"; exit 1; }
grep -q 'upload-artifact' "$WF" || { echo "FAIL: artifact upload lost"; exit 1; }
echo "release_ci_test.sh PASS"
