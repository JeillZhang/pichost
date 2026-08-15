#!/usr/bin/env bash
# packaging/common/install-lib.sh — 共享包安装逻辑(deb postinst / rpm %post 共享;随包安装到 /usr/share/pichost/)
# 用法: source install-lib.sh
# 测试/冒烟覆盖:PICHOST_PKG_ROOT 为所有绝对路径前缀(默认空)
: "${PICHOST_PKG_ROOT:=}"
ETC_DIR="${PICHOST_PKG_ROOT}/etc/pichost"
VAR_DIR="${PICHOST_PKG_ROOT}/var/lib/pichost"
STATIC_DIR="${PICHOST_PKG_ROOT}/usr/share/pichost/web-ui"
ENV_FILE="$ETC_DIR/.env"
SVC_API="pichost-api"

ensure_pkg_user() {
    [ "$(id -u)" = "0" ] || return 0
    id pichost >/dev/null 2>&1 || useradd --system --home-dir /var/lib/pichost pichost
}

ensure_pkg_dirs() {
    mkdir -p "$ETC_DIR" "$VAR_DIR" "$STATIC_DIR"
}

generate_pkg_env() {
    [ -f "$ENV_FILE" ] && return 0
    cat > "$ENV_FILE" <<EOF
PICHOST_DATABASE_MODE=sqlite
PICHOST_DATABASE_URL="sqlite://$VAR_DIR/pichost.db"
PICHOST_STORAGE__LOCAL_BASE_PATH="$VAR_DIR/storage-local"
PICHOST_STATIC_DIR=$STATIC_DIR
EOF
    chmod 600 "$ENV_FILE"
}

ensure_pkg_jwt() {
    local line secret new_secret
    line="$(grep -E '^PICHOST_AUTH(_|__)JWT_SECRET=' "$ENV_FILE" | tail -n 1 || true)"
    secret="${line#*=}"; secret="${secret%\"}"; secret="${secret#\"}"
    [ -n "$secret" ] && [ "${#secret}" -ge 32 ] && return 0
    if command -v openssl >/dev/null 2>&1; then
        new_secret="$(openssl rand -hex 32)"
    else
        new_secret="$(tr -dc 'a-f0-9' < /dev/urandom | head -c 64 || true)"
    fi
    sed -i -e '/^PICHOST_AUTH_JWT_SECRET=/d' -e '/^PICHOST_AUTH__JWT_SECRET=/d' "$ENV_FILE"
    printf 'PICHOST_AUTH__JWT_SECRET=%s\n' "$new_secret" >> "$ENV_FILE"
    chmod 600 "$ENV_FILE"
}

set_pkg_ownership() {
    chown -R pichost:pichost "$VAR_DIR" "$ETC_DIR" 2>/dev/null || true
}

enable_pkg_services() {
    [ "$(id -u)" = "0" ] || return 0
    command -v systemctl >/dev/null 2>&1 || return 0
    [ -d /run/systemd/system ] || return 0
    systemctl daemon-reload
    systemctl enable --now "$SVC_API" >/dev/null 2>&1 || true
}
