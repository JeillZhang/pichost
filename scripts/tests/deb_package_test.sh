#!/usr/bin/env bash
# scripts/tests/deb_package_test.sh — deb 打包 FHS env 生成契约
set -euo pipefail
# 用法: bash scripts/tests/deb_package_test.sh
# 断言 packaging/common/install-lib.sh 的 FHS env 生成(共享给 deb postinst / rpm %post)
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# ① 语法检查
bash -n "$ROOT/packaging/common/install-lib.sh"
bash -n "$ROOT/packaging/deb/postinst"

# ② 功能:临时根下生成 FHS env
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
export PICHOST_PKG_ROOT="$TMP"
# shellcheck disable=SC1091
. "$ROOT/packaging/common/install-lib.sh"
ensure_pkg_dirs
generate_pkg_env
ensure_pkg_jwt

ENV_FILE="$TMP/etc/pichost/.env"
[ -f "$ENV_FILE" ] || { echo "FAIL: .env missing"; exit 1; }
grep -q '^PICHOST_DATABASE_MODE=sqlite' "$ENV_FILE" \
  || { echo "FAIL: mode not sqlite"; exit 1; }
grep -Fq "sqlite://$TMP/var/lib/pichost/pichost.db" "$ENV_FILE" \
  || { echo "FAIL: db url wrong"; exit 1; }
grep -Fq "PICHOST_STATIC_DIR=$TMP/usr/share/pichost/web-ui" "$ENV_FILE" \
  || { echo "FAIL: static dir missing"; exit 1; }
[ -d "$TMP/var/lib/pichost" ] || { echo "FAIL: var dir missing"; exit 1; }

# ③ 幂等:JWT 有效时内容不变
cp "$ENV_FILE" "$TMP/env.before"
generate_pkg_env
ensure_pkg_jwt
diff -q "$TMP/env.before" "$ENV_FILE" >/dev/null \
  || { echo "FAIL: rerun not idempotent"; exit 1; }

# ④ JWT 长度断言
jwt="$(grep -E '^PICHOST_AUTH(_|__)JWT_SECRET=' "$ENV_FILE" | tail -n 1)"
secret="${jwt#*=}"; secret="${secret%\"}"; secret="${secret#\"}"
[ "${#secret}" -ge 32 ] || { echo "FAIL: JWT too short"; exit 1; }

# ⑤ postinst 内容断言
grep -q 'source /usr/share/pichost/install-lib.sh' "$ROOT/packaging/deb/postinst" \
  || { echo "FAIL: postinst missing lib source"; exit 1; }

# ⑥ prerm/postrm 语法与分支断言
bash -n "$ROOT/packaging/deb/prerm"
bash -n "$ROOT/packaging/deb/postrm"
grep -q 'systemctl stop pichost-api pichost-worker' "$ROOT/packaging/deb/prerm" \
  || { echo "FAIL: prerm missing stop"; exit 1; }
grep -q 'purge' "$ROOT/packaging/deb/postrm" \
  || { echo "FAIL: postrm missing purge branch"; exit 1; }
grep -q 'rm -rf /var/lib/pichost' "$ROOT/packaging/deb/postrm" \
  || { echo "FAIL: postrm missing data wipe"; exit 1; }
grep -q 'upgrade' "$ROOT/packaging/deb/postrm" \
  || { echo "FAIL: postrm missing upgrade guard"; exit 1; }

# ⑦ cargo-deb 元数据断言
grep -q '\[package.metadata.deb\]' "$ROOT/pichost-api/Cargo.toml" \
  || { echo "FAIL: deb metadata missing"; exit 1; }
grep -q 'maintainer-scripts = "packaging/deb"' "$ROOT/pichost-api/Cargo.toml" \
  || { echo "FAIL: maintainer-scripts missing"; exit 1; }
grep -q 'web-ui/dist' "$ROOT/pichost-api/Cargo.toml" \
  || { echo "FAIL: web-ui asset missing"; exit 1; }
grep -q 'usr/share/pichost/web-ui' "$ROOT/pichost-api/Cargo.toml" \
  || { echo "FAIL: web-ui dest missing"; exit 1; }
grep -q 'pichost-api.service' "$ROOT/pichost-api/Cargo.toml" \
  || { echo "FAIL: systemd unit asset missing"; exit 1; }
echo "deb_package_test.sh PASS"
