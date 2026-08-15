#!/usr/bin/env bash
# verify-release.sh — 本地模拟 .github/workflows/release.yml 的 build + package 流程。
#
# 与 CI 完全相同的命令序列（前端构建 → release 构建 → strip → test → clippy → 打包），
# 并对产物做布局校验、二进制冒烟测试和 install.sh 安装 dry-run。
# 打 tag 发布前运行一次，可覆盖 CI 中除 GitHub 专属步骤（actions/tag 触发/Release 创建）外的全部环节。
#
# 用法:
#   scripts/verify-release.sh [VERSION] [--skip-test] [--skip-lint] [--skip-install]
#   VERSION 默认取 Cargo.toml 的 workspace 版本（如 v0.17.5），与 tag 名保持一致。
#
# 产物:
#   dist/pichost-<VERSION>-amd64.tar.gz   （与 CI 的 Package 步骤输出一致，供人工检查）
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="x86_64-unknown-linux-gnu"
ARCH="amd64"
DIST_DIR="$ROOT/dist"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

usage() {
    echo "Usage: $0 [VERSION] [--skip-test] [--skip-lint] [--skip-install]"
    echo "  VERSION: tag 版本号，如 v0.17.5（默认取 Cargo.toml 版本）"
}

# --- 参数解析 ---
SKIP_TEST=0
SKIP_LINT=0
SKIP_INSTALL=0
VERSION=""
while [ $# -gt 0 ]; do
    case "$1" in
        --skip-test) SKIP_TEST=1 ;;
        --skip-lint) SKIP_LINT=1 ;;
        --skip-install) SKIP_INSTALL=1 ;;
        -h|--help) usage; exit 0 ;;
        -*) echo "unknown option: $1"; usage; exit 1 ;;
        *) VERSION="$1" ;;
    esac
    shift
done
VERSION="${VERSION:-v$(grep -m1 '^version' "$ROOT/Cargo.toml" | awk '{print $3}' | tr -d '"')}"
PKG_NAME="pichost-${VERSION}-${ARCH}"
PKG_DIR="$WORK_DIR/$PKG_NAME"

echo "=============================================="
echo " PicHost release verification"
echo "   version : $VERSION"
echo "   target  : $TARGET"
echo "   pkg     : $PKG_NAME"
echo "=============================================="

# --- [1/7] 前端构建（与 CI: cd web-ui && npm ci && npm run build 一致） ---
echo ""
echo "==> [1/7] Frontend build (npm ci && npm run build)"
(cd "$ROOT/web-ui" && npm ci && npm run build)

# --- [2/7] 后端 release 构建 + strip（与 CI 一致） ---
echo ""
echo "==> [2/7] Backend release build + strip"
cargo build --release --target "$TARGET" -p pichost-api -p pichost-worker
strip "$ROOT/target/$TARGET/release/pichost-api" "$ROOT/target/$TARGET/release/pichost-worker"

# --- [3/7] 测试（与 CI: cargo test --workspace 一致） ---
if [ "$SKIP_TEST" -eq 0 ]; then
    echo ""
    echo "==> [3/7] cargo test --workspace"
    cargo test --workspace
else
    echo ""
    echo "==> [3/7] cargo test --workspace (skipped)"
fi

# --- [4/7] Lint（与 CI: cargo clippy --workspace -- -D warnings 一致） ---
if [ "$SKIP_LINT" -eq 0 ]; then
    echo ""
    echo "==> [4/7] cargo clippy --workspace -- -D warnings"
    cargo clippy --workspace -- -D warnings
else
    echo ""
    echo "==> [4/7] cargo clippy --workspace -- -D warnings (skipped)"
fi

# --- [5/7] 打包（逐行复刻 release.yml 的 Package 步骤） ---
echo ""
echo "==> [5/7] Package (mirror of release.yml Package step)"
rm -rf "$DIST_DIR/$PKG_NAME"
mkdir -p "$DIST_DIR/$PKG_NAME/web-ui" "$DIST_DIR/$PKG_NAME/scripts"
cp "$ROOT/target/$TARGET/release/pichost-api" "$DIST_DIR/$PKG_NAME/"
cp "$ROOT/target/$TARGET/release/pichost-worker" "$DIST_DIR/$PKG_NAME/"
cp -r "$ROOT/web-ui/dist" "$DIST_DIR/$PKG_NAME/web-ui/"
cp -r "$ROOT/migrations" "$DIST_DIR/$PKG_NAME/"
cp -r "$ROOT/migrations-sqlite" "$DIST_DIR/$PKG_NAME/"
cp -r "$ROOT/nginx" "$DIST_DIR/$PKG_NAME/"
cp "$ROOT/.env.example" "$DIST_DIR/$PKG_NAME/"
cp "$ROOT/scripts/install.sh" "$ROOT/scripts/uninstall.sh" "$DIST_DIR/$PKG_NAME/scripts/"
cp "$ROOT/scripts/pichost-api.service" "$ROOT/scripts/pichost-worker.service" "$DIST_DIR/$PKG_NAME/scripts/"
cp "$ROOT/README.md" "$DIST_DIR/$PKG_NAME/"
(cd "$DIST_DIR" && tar czf "${PKG_NAME}.tar.gz" "$PKG_NAME")

# --- [6/7] 产物校验：解包 + 布局检查 + 二进制冒烟 ---
echo ""
echo "==> [6/7] Verify package layout"
mkdir -p "$PKG_DIR"
tar xzf "$DIST_DIR/${PKG_NAME}.tar.gz" -C "$WORK_DIR"
for f in \
    pichost-api \
    pichost-worker \
    web-ui/dist/index.html \
    migrations/0001_create_users.sql \
    migrations/0010_add_watermark_config.sql \
    nginx/nginx.conf \
    .env.example \
    README.md \
    scripts/install.sh \
    scripts/uninstall.sh \
    scripts/pichost-api.service \
    scripts/pichost-worker.service; do
    if [ ! -f "$PKG_DIR/$f" ]; then
        echo "FAIL: missing $f in package"
        exit 1
    fi
done
echo "layout OK (12 files checked)"

echo ""
echo "==> Binary smoke test"
file "$PKG_DIR/pichost-api" "$PKG_DIR/pichost-worker"
if ldd "$PKG_DIR/pichost-api" "$PKG_DIR/pichost-worker" 2>&1 | grep -q "not found"; then
    echo "FAIL: dynamic library missing"
    exit 1
fi
# 用不可达的数据库/Redis 地址启动：二进制应能加载并快速报连接错误，
# 而非段错误（139）或无法执行（126/127）。
check_rc() {
    local rc="$1" label="$2"
    case "$rc" in
        124) echo "OK  : $label started and kept running (killed by timeout)" ;;
        0)   echo "OK  : $label exited cleanly" ;;
        *)   if [ "$rc" -ge 126 ]; then
                 echo "FAIL: $label crashed (exit $rc)"
                 exit 1
             fi
             echo "OK  : $label exited with expected startup error (exit $rc)" ;;
    esac
}
check_binary() {
    local bin="$1" url="$2" label="$3"
    set +e
    PICHOST_DATABASE_URL="$url" PICHOST_REDIS_URL="$url" timeout 8 "$bin" >/dev/null 2>&1
    local rc=$?
    set -e
    check_rc "$rc" "$label"
}
# sqlite 模式冒烟（T28）: PICHOST_DATABASE_MODE=sqlite + 临时文件 URL，无需 Redis
# （PICHOST_REDIS_URL 置空）。预期: 迁移自动应用后进入 serve，进程保持运行（rc=124 OK）。
check_binary_sqlite() {
    local bin="$1" label="$2"
    set +e
    PICHOST_DATABASE_MODE=sqlite \
        PICHOST_DATABASE_URL="sqlite://$(mktemp -d)/smoke.db" \
        PICHOST_REDIS_URL= \
        timeout 8 "$bin" >/dev/null 2>&1
    local rc=$?
    set -e
    check_rc "$rc" "$label"
}
check_binary "$PKG_DIR/pichost-api" "postgres://127.0.0.1:9/pichost" "pichost-api"
check_binary "$PKG_DIR/pichost-worker" "redis://127.0.0.1:9/" "pichost-worker"
check_binary_sqlite "$PKG_DIR/pichost-api" "pichost-api (sqlite lite mode)"

# --- [7/7] install.sh dry-run（优先在无 systemd 的容器中模拟裸机安装） ---
if [ "$SKIP_INSTALL" -eq 0 ]; then
    echo ""
    echo "==> [7/7] install.sh dry-run"
    if command -v docker &>/dev/null; then
        echo ">> running install.sh inside a systemd-free container (ubuntu:24.04)"
        docker run --rm -v "$PKG_DIR:/$PKG_NAME:ro" ubuntu:24.04 \
            bash -c "cd /$PKG_NAME && bash scripts/install.sh /opt/pichost /etc/pichost \
                && test -d /opt/pichost/data \
                && grep -q '^PICHOST_DATABASE_MODE=sqlite' /etc/pichost/.env \
                && grep -q 'sqlite:///opt/pichost/data/pichost.db' /etc/pichost/.env"
    elif ! command -v systemctl &>/dev/null || [ "$(id -u)" = "0" ]; then
        (cd "$PKG_DIR" && bash scripts/install.sh /opt/pichost /etc/pichost)
    else
        echo ">> SKIP: host has systemd and is not root; run 'sudo bash $0' or use Docker for the install dry-run"
    fi
fi

# --- [8/8] deb 构建 + 安装冒烟(SQLite + 静态服务即开即用;缺工具不阻断) ---
echo ""
echo "==> [8/8] deb smoke (cargo deb build + dpkg install in container)"
if command -v cargo-deb >/dev/null 2>&1 && command -v docker >/dev/null 2>&1; then
    cargo deb -p pichost-api --no-build --output "$DIST_DIR/pichost-smoke.deb"
    docker run --rm -v "$DIST_DIR:/pkg:ro" ubuntu:24.04 bash -c "
        set -e
        apt-get update -qq && apt-get install -y -qq sqlite3 >/dev/null 2>&1 || true
        dpkg -i /pkg/pichost-smoke.deb
        mkdir -p /tmp/smoke && chown -R pichost:pichost /tmp/smoke
        su -s /bin/bash pichost -c 'PICHOST_DATABASE_MODE=sqlite PICHOST_DATABASE_URL=sqlite:///tmp/smoke/pichost.db \
            PICHOST_STORAGE__LOCAL_BASE_PATH=/tmp/smoke/storage PICHOST_AUTH__JWT_SECRET=0123456789abcdef0123456789abcdef \
            PICHOST_STATIC_DIR=/usr/share/pichost/web-ui PICHOST_SERVER_PUBLIC_URL=http://localhost:3000 \
            /usr/bin/pichost-api &'
        sleep 4
        curl -fsS http://localhost:3000/api/health | grep -q ok
        curl -fsS http://localhost:3000/ | grep -qi '<!doctype html'
    "
else
    echo "WARN: cargo-deb and/or docker not installed; deb smoke skipped"
fi

echo ""
echo "=============================================="
echo " VERIFICATION PASSED"
echo " artifact: $DIST_DIR/${PKG_NAME}.tar.gz"
echo "=============================================="
