#!/usr/bin/env bash
set -euo pipefail
# verify_release_test.sh — verify-release.sh sqlite 冒烟 + .env.example 变量断言（T28）
# 用法: bash scripts/tests/verify_release_test.sh
# 断言:
#   - verify-release.sh 含 sqlite 冒烟分支（PICHOST_DATABASE_MODE=sqlite + 临时文件 URL）
#   - .env.example 含 PICHOST_DATABASE_MODE 与 i18n 变量
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# 断言 verify-release.sh 含 sqlite 冒烟分支（check_binary_sqlite 或 PICHOST_DATABASE_MODE=sqlite 出现）
grep -q 'PICHOST_DATABASE_MODE=sqlite' "$ROOT/scripts/verify-release.sh" \
  || { echo "FAIL: verify-release.sh missing sqlite smoke"; exit 1; }
grep -q 'sqlite://' "$ROOT/scripts/verify-release.sh" \
  || { echo "FAIL: verify-release.sh missing sqlite temp-file URL"; exit 1; }
# 断言 .env.example 含新变量
grep -q 'PICHOST_DATABASE_MODE' "$ROOT/.env.example" \
  || { echo "FAIL: .env.example missing PICHOST_DATABASE_MODE"; exit 1; }
grep -q 'PICHOST_I18N_LANGUAGE' "$ROOT/.env.example" \
  || { echo "FAIL: .env.example missing PICHOST_I18N_LANGUAGE"; exit 1; }
grep -q 'PICHOST_I18N_LOCALES_DIR' "$ROOT/.env.example" \
  || { echo "FAIL: .env.example missing PICHOST_I18N_LOCALES_DIR"; exit 1; }
echo "verify_release_test.sh PASS"
