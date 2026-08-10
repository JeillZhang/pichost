#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
grep -m1 '^version' "$ROOT/Cargo.toml" | grep -q '0.21.0'
grep -m1 '"version"' "$ROOT/web-ui/package.json" | grep -q '0.21.0'
echo "version_check_test.sh PASS"
