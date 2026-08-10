#!/usr/bin/env bash
# docs_check_test.sh — assert AGENTS.md / README.md are in sync with the
# SQLite lite mode feature (v0.21.0). Part of the T29 docs-sync gate.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
grep -q 'PICHOST_DATABASE_MODE' "$ROOT/README.md"
grep -q 'sqlite' "$ROOT/AGENTS.md"
grep -q '0.21.0' "$ROOT/CHANGELOG.md"
grep -q '轻量模式' "$ROOT/.omo/summary/summary_and_next.md"
echo "docs_check_test.sh PASS"
