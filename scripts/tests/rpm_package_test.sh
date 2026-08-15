#!/usr/bin/env bash
set -euo pipefail
# 用法: bash scripts/tests/rpm_package_test.sh
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# ① 语法
bash -n "$ROOT/packaging/rpm/postinstall.sh"
bash -n "$ROOT/packaging/common/install-lib.sh"

# ② rpm 元数据
grep -q '\[package.metadata.generate-rpm\]' "$ROOT/pichost-api/Cargo.toml" \
  || { echo "FAIL: generate-rpm metadata missing"; exit 1; }
grep -q 'postinstall_script = "packaging/rpm/postinstall.sh"' "$ROOT/pichost-api/Cargo.toml" \
  || { echo "FAIL: postinstall_script missing"; exit 1; }
grep -q 'preuninstall_script' "$ROOT/pichost-api/Cargo.toml" \
  || { echo "FAIL: preuninstall_script missing"; exit 1; }
grep -q 'postuninstall_script' "$ROOT/pichost-api/Cargo.toml" \
  || { echo "FAIL: postuninstall_script missing"; exit 1; }
grep -q 'usr/lib/systemd/system' "$ROOT/pichost-api/Cargo.toml" \
  || { echo "FAIL: rpm systemd units missing"; exit 1; }

# ③ %post 内容
grep -q 'source /usr/share/pichost/install-lib.sh' "$ROOT/packaging/rpm/postinstall.sh" \
  || { echo "FAIL: %post missing lib source"; exit 1; }
grep -q 'ensure_pkg_jwt' "$ROOT/packaging/rpm/postinstall.sh" \
  || { echo "FAIL: %post missing jwt"; exit 1; }
grep -q 'try-restart' "$ROOT/packaging/rpm/postinstall.sh" \
  || { echo "FAIL: %post missing upgrade restart"; exit 1; }

# ④ preun/postun 断言
bash -n "$ROOT/packaging/rpm/preuninstall.sh"
bash -n "$ROOT/packaging/rpm/postuninstall.sh"
grep -q '"$1" = "0"' "$ROOT/packaging/rpm/preuninstall.sh" \
  || { echo "FAIL: preun missing uninstall guard"; exit 1; }
grep -q 'systemctl stop' "$ROOT/packaging/rpm/preuninstall.sh" \
  || { echo "FAIL: preun missing stop"; exit 1; }
grep -q '"$1" = "0"' "$ROOT/packaging/rpm/postuninstall.sh" \
  || { echo "FAIL: postun missing uninstall guard"; exit 1; }
grep -q 'rm -rf /var/lib/pichost' "$ROOT/packaging/rpm/postuninstall.sh" \
  || { echo "FAIL: postun missing data wipe"; exit 1; }
echo "rpm_package_test.sh PASS"
