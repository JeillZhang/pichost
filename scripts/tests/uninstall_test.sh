# scripts/tests/uninstall_test.sh — uninstall.sh 单目录契约断言
#!/usr/bin/env bash
set -euo pipefail
# 用法: bash scripts/tests/uninstall_test.sh <pkg_dir>
# 断言:
#   - 默认(非 tty):INSTALL_DIR 整体删除(含 data/ 图片数据)
#   - --keep-data:INSTALL_DIR 保留且 data/ 存在,二进制已删除
#   - CONFIG_DIR 保留(默认策略)
PKG_DIR="${1:?usage: uninstall_test.sh <pkg_dir>}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# ① 安装现场(sqlite 单目录契约,来自 T0)
(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode sqlite "$TMP/pi" "$TMP/pc")
[ -d "$TMP/pi/data" ] || { echo "FAIL: setup install.sh (data/ missing)"; exit 1; }

# ② 默认卸载:整体清除(含 data/)
bash "$PKG_DIR/scripts/uninstall.sh" "$TMP/pi" "$TMP/pc"
[ ! -d "$TMP/pi" ] || { echo "FAIL: INSTALL_DIR not removed"; exit 1; }
[ -d "$TMP/pc" ] || { echo "FAIL: CONFIG_DIR should be preserved"; exit 1; }
echo "uninstall_test.sh: default wipe OK"

# ③ 重新安装 → --keep-data 卸载
(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode sqlite "$TMP/pi" "$TMP/pc")
bash "$PKG_DIR/scripts/uninstall.sh" --keep-data "$TMP/pi" "$TMP/pc"
[ -d "$TMP/pi/data" ] || { echo "FAIL: --keep-data lost data/"; exit 1; }
[ ! -f "$TMP/pi/pichost-api" ] || { echo "FAIL: binary still present"; exit 1; }
[ ! -f "$TMP/pi/web-ui/dist/index.html" ] || { echo "FAIL: static assets still present"; exit 1; }
echo "uninstall_test.sh: --keep-data OK"
echo "uninstall_test.sh PASS"
