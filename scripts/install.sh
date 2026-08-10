#!/bin/bash
# install.sh — PicHost bare-metal installer (interactive)
#
# 用法:
#   install.sh [--yes] [--mode postgres|sqlite] [INSTALL_DIR] [DATA_DIR] [CONFIG_DIR]
#   --yes                无人值守：跳过所有交互提问（--mode 缺省时自动检测）
#   --mode postgres|sqlite  强制指定数据库模式（缺省自动检测：有 pg_isready → postgres）
#   INSTALL_DIR/DATA_DIR/CONFIG_DIR 安装路径（默认 /opt/pichost /var/lib/pichost /etc/pichost）
#
# 无参数运行时保持原有安装行为，仅新增缺失依赖的交互引导（tty 且未 --yes 时）。
# SQLite 模式：零外部依赖（内嵌 worker），.env 使用 sqlite://，systemd 单元去掉
# postgresql/redis 依赖，且不安装 pichost-worker.service。
set -euo pipefail

ASSUME_YES=0
MODE=""
INSTALL_DIR="/opt/pichost"
DATA_DIR="/var/lib/pichost"
CONFIG_DIR="/etc/pichost"

usage() {
    echo "Usage: $0 [--yes] [--mode postgres|sqlite] [INSTALL_DIR] [DATA_DIR] [CONFIG_DIR]"
    echo "  --yes                  unattended install (skip prompts)"
    echo "  --mode postgres|sqlite force database mode (default: auto-detect)"
}

# --- 参数解析（保持原有位置参数契约） ---
POS=0
while [ $# -gt 0 ]; do
    case "$1" in
        --yes) ASSUME_YES=1 ;;
        --mode)
            [ $# -ge 2 ] || { echo "error: --mode requires an argument"; usage; exit 1; }
            case "$2" in
                postgres|sqlite) MODE="$2" ;;
                *) echo "error: --mode must be postgres|sqlite (got: $2)"; exit 1 ;;
            esac
            shift ;;
        -h|--help) usage; exit 0 ;;
        -*) echo "unknown option: $1"; usage; exit 1 ;;
        *)
            POS=$((POS + 1))
            case "$POS" in
                1) INSTALL_DIR="$1" ;;
                2) DATA_DIR="$1" ;;
                3) CONFIG_DIR="$1" ;;
                *) echo "error: too many positional arguments: $1"; usage; exit 1 ;;
            esac ;;
    esac
    shift
done

PKG_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${PICHOST_VERSION:-$(basename "$PKG_DIR" | sed -nE 's/^pichost-v([0-9]+\.[0-9]+\.[0-9]+)-.*$/\1/p')}"
VERSION="${VERSION:-unknown}"

# --- 依赖检测 ---
has_pg() { command -v pg_isready >/dev/null 2>&1; }
has_redis() { command -v redis-cli >/dev/null 2>&1; }
is_tty() { [ -t 0 ]; }

# apt 自动安装（仅 root + Debian/Ubuntu）
apt_install() {
    if [ "$(id -u)" != "0" ] || ! command -v apt-get >/dev/null 2>&1; then
        echo ">> 无法自动安装: 需要 root 权限且为 Debian/Ubuntu 系统，请手动安装后重新运行" >&2
        exit 1
    fi
    apt-get update -qq
    apt-get install -y "$@"
}

# 模式解析：--mode 未指定时自动检测；无 PG 时交互引导（非 tty / --yes 默认 SQLite）
resolve_mode() {
    [ -n "$MODE" ] && return
    if has_pg; then
        MODE="postgres"
        return
    fi
    if [ "$ASSUME_YES" -eq 1 ] || ! is_tty; then
        echo ">> WARNING: PostgreSQL not detected (pg_isready missing); defaulting to SQLite mode"
        MODE="sqlite"
        return
    fi
    while true; do
        echo ">> PostgreSQL 未检测到 (需要 pg_isready)"
        echo "   [1] 自动安装 PostgreSQL (apt, 仅 Debian/Ubuntu)"
        echo "   [2] 改用 SQLite 模式 (零依赖, 推荐轻量)"
        echo "   [3] 手动安装后重跑"
        if ! read -r -p "请选择 [1/2/3]: " ans; then
            echo; echo ">> 输入已结束，退出"; exit 1
        fi
        case "$ans" in
            1)
                apt_install postgresql postgresql-client
                if command -v systemctl >/dev/null 2>&1; then
                    systemctl enable --now postgresql >/dev/null 2>&1 || true
                fi
                MODE="postgres"; break ;;
            2) MODE="sqlite"; break ;;
            3) echo ">> 请手动安装 PostgreSQL 后重新运行 install.sh"; exit 1 ;;
            *) echo ">> 无效选择: $ans" ;;
        esac
    done
}

# Redis 检查（仅 postgres 模式）
check_redis() {
    [ "$MODE" = "postgres" ] || return 0
    has_redis && return 0
    if [ "$ASSUME_YES" -eq 1 ] || ! is_tty; then
        echo ">> WARNING: Redis not detected (redis-cli missing); PICHOST_REDIS_URL must be reachable"
        return 0
    fi
    while true; do
        echo ">> Redis 未检测到 (需要 redis-cli)"
        echo "   [1] apt 自动安装 Redis"
        echo "   [2] 手动安装后重跑"
        if ! read -r -p "请选择 [1/2]: " ans; then
            echo; echo ">> 输入已结束，退出"; exit 1
        fi
        case "$ans" in
            1)
                apt_install redis-server
                if command -v systemctl >/dev/null 2>&1; then
                    systemctl enable --now redis-server >/dev/null 2>&1 || true
                fi
                break ;;
            2) echo ">> 请手动安装 Redis 后重新运行 install.sh"; exit 1 ;;
            *) echo ">> 无效选择: $ans" ;;
        esac
    done
}

# --- .env 按模式生成（首次复制 .env.example，随后按 mode 覆写关键变量） ---
generate_env() {
    local env_file="$CONFIG_DIR/.env"
    if [ ! -f "$env_file" ]; then
        cp .env.example "$env_file"
        echo ">> Created $env_file from .env.example"
    fi
    if [ "$MODE" = "sqlite" ]; then
        sed -i -e '/^PICHOST_DATABASE_MODE=/d' \
            -e '/^PICHOST_DATABASE_URL=/d' \
            -e '/^PICHOST_REDIS_URL=/d' "$env_file"
        {
            printf '# SQLite 模式: 零外部依赖 (内嵌 worker)\n'
            printf 'PICHOST_DATABASE_MODE=sqlite\n'
            printf 'PICHOST_DATABASE_URL="sqlite://%s/pichost.db"\n' "$DATA_DIR"
        } >> "$env_file"
        echo ">> SQLite mode configured: $DATA_DIR/pichost.db"
    else
        sed -i '/^PICHOST_DATABASE_MODE=/d' "$env_file"
        printf 'PICHOST_DATABASE_MODE=postgres\n' >> "$env_file"
        echo ">> PostgreSQL mode configured (edit $env_file for credentials)"
    fi
}

# --- JWT 校验：缺失或 <32 字符 → 生成随机 secret 并写入 ---
ensure_jwt_secret() {
    local env_file="$CONFIG_DIR/.env"
    local line secret
    line="$(grep -E '^PICHOST_AUTH(_|__)JWT_SECRET=' "$env_file" | tail -n 1 || true)"
    secret="${line#*=}"
    secret="${secret%\"}"; secret="${secret#\"}"
    if [ -n "$secret" ] && [ "${#secret}" -ge 32 ]; then
        return 0
    fi
    local new_secret
    if command -v openssl >/dev/null 2>&1; then
        new_secret="$(openssl rand -hex 32)"
    else
        new_secret="$(tr -dc 'a-f0-9' < /dev/urandom | head -c 64)"
    fi
    if [ -n "$secret" ]; then
        echo ">> WARNING: PICHOST_AUTH_JWT_SECRET ($((${#secret})) chars) is too short (<32); replacing it"
    else
        echo ">> PICHOST_AUTH_JWT_SECRET missing; generating a random secret"
    fi
    sed -i -e '/^PICHOST_AUTH_JWT_SECRET=/d' -e '/^PICHOST_AUTH__JWT_SECRET=/d' "$env_file"
    printf 'PICHOST_AUTH_JWT_SECRET=%s\n' "$new_secret" >> "$env_file"
    echo ">> Generated PICHOST_AUTH_JWT_SECRET written to $env_file"
}

# --- 创建 pichost 系统用户 + 目录权限（仅 root 可执行） ---
create_user_and_perms() {
    [ "$(id -u)" = "0" ] || { echo ">> (Not root: skipping pichost user creation and chown)"; return 0; }
    if ! id pichost >/dev/null 2>&1; then
        useradd --system --home "$INSTALL_DIR" pichost
        echo ">> Created system user 'pichost'"
    fi
    chown -R pichost:pichost "$INSTALL_DIR" "$DATA_DIR" "$CONFIG_DIR"
    echo ">> Ownership set: pichost:pichost on $INSTALL_DIR $DATA_DIR $CONFIG_DIR"
}

# --- systemd 单元生成（模板 sed 条件化；SQLite 去掉 postgresql/redis 依赖、不装 worker） ---
generate_units() {
    local TMP_SVC
    TMP_SVC="$(mktemp -d)"
    cp scripts/pichost-api.service "$TMP_SVC/"
    if [ "$MODE" = "postgres" ]; then
        cp scripts/pichost-worker.service "$TMP_SVC/"
    fi
    sed -i "s|/opt/pichost|$INSTALL_DIR|g" "$TMP_SVC"/*.service
    sed -i "s|/etc/pichost|$CONFIG_DIR|g" "$TMP_SVC"/*.service
    if [ "$MODE" = "sqlite" ]; then
        # 轻量模式: 删除 Wants 行并摘掉 After 上的 postgresql/redis 依赖（保留 network.target）
        sed -i -e '/Wants=.*\(postgresql\|redis\)/d' \
            -e 's/ postgresql\.service//g' \
            -e 's/ redis\.service//g' \
            -e 's/[[:space:]]*$//' "$TMP_SVC"/pichost-api.service
    fi
    # 处理后的单元始终写入 CONFIG_DIR（便于检查）；root + 运行中的 systemd 时再安装到系统目录
    cp "$TMP_SVC"/*.service "$CONFIG_DIR/"
    if [ "$(id -u)" = "0" ] && command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
        cp "$TMP_SVC"/*.service /etc/systemd/system/
        systemctl daemon-reload
        echo ">> systemd services installed"
        echo ">> Start:  systemctl start pichost-api"
        [ "$MODE" = "postgres" ] && echo ">>          systemctl start pichost-worker"
        echo ">> Enable: systemctl enable pichost-api"
        [ "$MODE" = "postgres" ] && echo ">>          systemctl enable pichost-worker"
    else
        echo ">> (Non-systemd or not root; generated units saved to $CONFIG_DIR)"
        echo ">> API:    $INSTALL_DIR/pichost-api"
        [ "$MODE" = "postgres" ] && echo ">> Worker: $INSTALL_DIR/pichost-worker"
    fi
    rm -rf "$TMP_SVC"
}

# --- 主流程 ---
echo "PicHost v${VERSION} installing..."

resolve_mode
check_redis

# 1. Create directory structure
mkdir -p "$INSTALL_DIR" "$DATA_DIR" "$CONFIG_DIR"

# 2. Copy binaries
cp pichost-api pichost-worker "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR"/pichost-api "$INSTALL_DIR"/pichost-worker

# 3. Copy static assets
cp -r web-ui/dist "$INSTALL_DIR/"
cp -r migrations "$INSTALL_DIR/"
if [ -d nginx ]; then cp -r nginx "$INSTALL_DIR/"; fi

# 4. Initialize .env (mode-aware) + validate JWT secret
generate_env
ensure_jwt_secret

# 5. Prerequisite check
if [ "$MODE" = "postgres" ]; then
    echo ">> Ensure PostgreSQL 18+ and Redis 8+ are installed and running"
else
    echo ">> SQLite mode: no external database/Redis required (embedded worker)"
fi

# 6. pichost user + directory permissions
create_user_and_perms

# 7. Install systemd services (conditional on mode)
generate_units

echo "PicHost installation complete."
