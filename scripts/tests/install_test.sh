# scripts/tests/install_test.sh — 单目录契约 + sqlite 默认断言(重写)
#!/usr/bin/env bash
set -euo pipefail
# 用法: bash scripts/tests/install_test.sh <pkg_dir>
# 断言(0.22.0 单目录契约):
#   - 双位置参数 [INSTALL_DIR] [CONFIG_DIR];sqlite URL 指向 $INSTALL_DIR/data/pichost.db
#   - .env 含 PICHOST_STORAGE_LOCAL_BASE_PATH=$INSTALL_DIR/data/storage-local
#   - 无 --mode 时默认 sqlite(非 tty);重跑幂等(关键行无重复)
#   - 既有断言保留:JWT >=32 字符、.env 权限 600、单元无 postgresql 依赖
PKG_DIR="${1:?usage: install_test.sh <pkg_dir>}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# ① 显式 sqlite + 双位置参数(旧 3 参契约改为 2 参)
(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode sqlite "$TMP/pi" "$TMP/pc")

grep -q 'PICHOST_DATABASE_MODE=sqlite' "$TMP/pc/.env"
grep -Fq "sqlite://$TMP/pi/data/pichost.db" "$TMP/pc/.env"
grep -Fq "PICHOST_STORAGE_LOCAL_BASE_PATH=\"$TMP/pi/data/storage-local\"" "$TMP/pc/.env"
[ -d "$TMP/pi/data" ] || { echo "FAIL: data/ dir missing"; exit 1; }
! grep -q 'Wants=.*postgresql' "$TMP/pc/pichost-api.service"

# ② 默认模式(无 --mode,非 tty)→ sqlite
(cd "$PKG_DIR" && bash scripts/install.sh --yes "$TMP/pi2" "$TMP/pc2")
grep -q 'PICHOST_DATABASE_MODE=sqlite' "$TMP/pc2/.env"
grep -Fq "sqlite://$TMP/pi2/data/pichost.db" "$TMP/pc2/.env"

# ③ 幂等重跑:关键行各恰 1 次
(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode sqlite "$TMP/pi" "$TMP/pc")
[ "$(grep -c '^PICHOST_DATABASE_MODE=' "$TMP/pc/.env")" -eq 1 ] || { echo "FAIL: dup MODE line"; exit 1; }
[ "$(grep -c '^PICHOST_STORAGE_LOCAL_BASE_PATH=' "$TMP/pc/.env")" -eq 1 ] || { echo "FAIL: dup STORAGE line"; exit 1; }
[ "$(grep -c '^PICHOST_DATABASE_URL=' "$TMP/pc/.env")" -eq 1 ] || { echo "FAIL: dup URL line"; exit 1; }

# ④ postgres 模式:URL 必须被清理/替换,不得残留 sqlite URL(防止 .env.example 翻转后串配置)
(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode postgres "$TMP/pi3" "$TMP/pc3")
grep -q 'PICHOST_DATABASE_MODE=postgres' "$TMP/pc3/.env" \
  || { echo "FAIL: postgres mode not set"; exit 1; }
! grep -q '^PICHOST_DATABASE_URL=sqlite://' "$TMP/pc3/.env" \
  || { echo "FAIL: postgres mode left sqlite URL"; exit 1; }

# ⑤ 既有断言(保留)
jwt_line="$(grep -E '^PICHOST_AUTH(_|__)JWT_SECRET=' "$TMP/pc/.env" | tail -n 1)"
jwt_secret="${jwt_line#*=}"; jwt_secret="${jwt_secret%\"}"; jwt_secret="${jwt_secret#\"}"
[ "${#jwt_secret}" -ge 32 ] || { echo "FAIL: JWT too short"; exit 1; }
[ "$(stat -c '%a' "$TMP/pc/.env")" = "600" ] || { echo "FAIL: .env perms not 600"; exit 1; }
echo "install_test.sh PASS"
