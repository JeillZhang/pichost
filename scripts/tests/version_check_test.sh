# scripts/tests/version_check_test.sh — 版本对齐断言(0.22.0)
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
grep -q '^version = "0.22.0"' "$ROOT/Cargo.toml" \
  || { echo "FAIL: Cargo.toml not 0.22.0"; exit 1; }
grep -q '"version": "0.22.0"' "$ROOT/web-ui/package.json" \
  || { echo "FAIL: package.json not 0.22.0"; exit 1; }
grep -q '"version": "0.22.0"' "$ROOT/web-ui/package-lock.json" \
  || { echo "FAIL: package-lock.json not 0.22.0"; exit 1; }
grep -A1 'name = "pichost-api"' "$ROOT/Cargo.lock" | grep -q '0.22.0' \
  || { echo "FAIL: Cargo.lock pichost-api not 0.22.0"; exit 1; }
echo "version_check_test.sh PASS"
