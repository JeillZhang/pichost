#!/usr/bin/env bash
# docs_check_test.sh — assert AGENTS.md / README.md are in sync with the
# SQLite lite mode feature (v0.21.0) + 0.22.0 sqlite-first deployment docs.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# 既有断言(保留):lite mode 文档同步;CHANGELOG/summary greps 有效至 T5
grep -q 'PICHOST_DATABASE_MODE' "$ROOT/README.md"
grep -q 'sqlite' "$ROOT/AGENTS.md"
grep -q '0.21.0' "$ROOT/CHANGELOG.md"
grep -q '轻量模式' "$ROOT/.omo/summary/summary_and_next.md"
# 新增 0.22.0 部署文档断言
grep -q 'data/pichost.db' "$ROOT/README.md" \
  || { echo "FAIL: README missing data/ sqlite path"; exit 1; }
grep -q '\[INSTALL_DIR\] \[CONFIG_DIR\]' "$ROOT/AGENTS.md" \
  || { echo "FAIL: AGENTS missing new install.sh signature"; exit 1; }
grep -q '0.22.0' "$ROOT/AGENTS.md" \
  || { echo "FAIL: AGENTS version not 0.22.0"; exit 1; }
grep -q '0.22.0' "$ROOT/CHANGELOG.md" \
  || { echo "FAIL: CHANGELOG missing 0.22.0"; exit 1; }
grep -q 'SQLite 优先' "$ROOT/.omo/summary/summary_and_next.md" \
  || { echo "FAIL: summary missing sqlite-first section"; exit 1; }
# 原生安装包文档断言 (v0.23.0 native packaging)
grep -q 'apt install pichost' "$ROOT/README.md" \
  || { echo "FAIL: README missing apt install"; exit 1; }
grep -q 'brew install pichost' "$ROOT/README.md" \
  || { echo "FAIL: README missing brew install"; exit 1; }
grep -q 'winget install PicHost.PicHost' "$ROOT/README.md" \
  || { echo "FAIL: README missing winget"; exit 1; }
grep -q 'PICHOST_STATIC_DIR' "$ROOT/README.md" \
  || { echo "FAIL: README missing static dir config"; exit 1; }
grep -q '0.23.0' "$ROOT/AGENTS.md" \
  || { echo "FAIL: AGENTS version not 0.23.0"; exit 1; }
grep -q 'PICHOST_STATIC_DIR' "$ROOT/AGENTS.md" \
  || { echo "FAIL: AGENTS missing static dir"; exit 1; }
grep -q 'packaging/' "$ROOT/AGENTS.md" \
  || { echo "FAIL: AGENTS missing packaging dir"; exit 1; }
# 0.23.0 收尾断言: CHANGELOG 条目 + summary 本次完成小节
grep -q '0.23.0' "$ROOT/CHANGELOG.md" \
  || { echo "FAIL: CHANGELOG missing 0.23.0"; exit 1; }
grep -q '原生安装包' "$ROOT/.omo/summary/summary_and_next.md" \
  || { echo "FAIL: summary missing phase section"; exit 1; }
echo "docs_check_test.sh PASS"
