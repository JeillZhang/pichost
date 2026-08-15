# scripts/tests/version_check_test.sh — 改写为 1.0.0 断言(现为 0.24.0)
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
grep -q '^version = "1.0.0"' "$ROOT/Cargo.toml" \
  || { echo "FAIL: Cargo.toml not 1.0.0"; exit 1; }
grep -q '"version": "1.0.0"' "$ROOT/web-ui/package.json" \
  || { echo "FAIL: package.json not 1.0.0"; exit 1; }
grep -q '"version": "1.0.0"' "$ROOT/web-ui/package-lock.json" \
  || { echo "FAIL: package-lock.json not 1.0.0"; exit 1; }
grep -A1 'name = "pichost-api"' "$ROOT/Cargo.lock" | grep -q '1.0.0' \
  || { echo "FAIL: Cargo.lock pichost-api not 1.0.0"; exit 1; }
echo "version_check_test.sh PASS"
