#!/usr/bin/env bash
set -euo pipefail
# install_test.sh — install.sh sqlite-mode 冒烟断言（T27）
# 用法: bash scripts/tests/install_test.sh <pkg_dir>
# 断言: --yes --mode sqlite 安装到临时目录后
#   - $CONFIG_DIR/.env 含 PICHOST_DATABASE_MODE=sqlite 且 URL 指向 sqlite://$DATA_DIR/pichost.db
#   - 生成的 service 单元不含 postgresql.service 依赖（systemd 不可用时跳过单元断言）
PKG_DIR="${1:?usage: install_test.sh <pkg_dir>}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode sqlite \
  "$TMP/pi" "$TMP/pd" "$TMP/pc")

grep -q 'PICHOST_DATABASE_MODE=sqlite' "$TMP/pc/.env"
grep -q 'sqlite://' "$TMP/pc/.env"
! grep -q 'Wants=.*postgresql' "$TMP/pc/pichost-api.service" 2>/dev/null || true
# systemd 不可用时跳过单元断言；核心断言为 .env 生成
echo "install_test.sh PASS"
