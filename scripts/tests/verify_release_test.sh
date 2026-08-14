#!/usr/bin/env bash
# scripts/tests/verify_release_test.sh — verify-release dry-run 契约 + .env.example 默认翻转断言
set -euo pipefail
# 用法: bash scripts/tests/verify_release_test.sh
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# ① verify-release.sh:install dry-run 为双位置参数(无 DATA_DIR)
grep -q 'scripts/install.sh /opt/pichost /etc/pichost' "$ROOT/scripts/verify-release.sh" \
  || { echo "FAIL: verify-release.sh dry-run not 2-arg"; exit 1; }
# ② 全脚本目录不得残留旧 DATA_DIR 路径(--exclude-dir=tests:本文件自身的 grep 模式不误伤)
! grep -rq --exclude-dir=tests '/var/lib/pichost' "$ROOT/scripts/" \
  || { echo "FAIL: stale /var/lib/pichost in scripts"; exit 1; }
# ③ .env.example 默认 sqlite(取消注释)+ data/ URL + 注释 Redis + storage 路径
grep -q '^PICHOST_DATABASE_MODE=sqlite' "$ROOT/.env.example" \
  || { echo "FAIL: .env.example not sqlite by default"; exit 1; }
grep -q 'sqlite:///opt/pichost/data/pichost.db' "$ROOT/.env.example" \
  || { echo "FAIL: .env.example missing data/ sqlite URL"; exit 1; }
grep -q '^# PICHOST_REDIS_URL' "$ROOT/.env.example" \
  || { echo "FAIL: .env.example Redis not commented"; exit 1; }
grep -q 'PICHOST_STORAGE__LOCAL_BASE_PATH=/opt/pichost/data/storage-local' "$ROOT/.env.example" \
  || { echo "FAIL: .env.example storage path not data/"; exit 1; }
echo "verify_release_test.sh PASS"
