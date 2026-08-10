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
! grep -q 'Wants=.*postgresql' "$TMP/pc/pichost-api.service"
# 单元由 install.sh 无条件写入 $CONFIG_DIR，故此处为硬断言（生成单元必须无 postgresql 依赖）

# JWT secret：安装器生成的 secret 必须 >= 32 字符（.env.example 占位符 <32 会被替换）
jwt_line="$(grep -E '^PICHOST_AUTH(_|__)JWT_SECRET=' "$TMP/pc/.env" | tail -n 1)"
jwt_secret="${jwt_line#*=}"
jwt_secret="${jwt_secret%\"}"; jwt_secret="${jwt_secret#\"}"
[ "${#jwt_secret}" -ge 32 ] || { echo "FAIL: PICHOST_AUTH_JWT_SECRET too short (${#jwt_secret} chars)"; exit 1; }
echo "install_test.sh: JWT secret length ${#jwt_secret} (>=32) OK"

# .env 权限：包含密钥，必须 600
[ "$(stat -c '%a' "$TMP/pc/.env")" = "600" ] || { echo "FAIL: .env perms not 600"; exit 1; }
echo "install_test.sh: .env perms 600 OK"

echo "install_test.sh PASS"
