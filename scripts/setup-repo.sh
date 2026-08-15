#!/bin/bash
# 一键配置 PicHost 软件仓库(apt / dnf 自动检测)
set -euo pipefail
REPO_BASE="${PICHOST_REPO_BASE:-https://jeillzhang.github.io/pichost-repo}"
if command -v apt-get >/dev/null 2>&1; then
    curl -fsSL "$REPO_BASE/public.key" \
        | sudo gpg --dearmor -o /usr/share/keyrings/pichost-archive-keyring.gpg
    echo "deb [signed-by=/usr/share/keyrings/pichost-archive-keyring.gpg] $REPO_BASE/apt stable main" \
        | sudo tee /etc/apt/sources.list.d/pichost.list >/dev/null
    sudo apt-get update -qq
    echo "Done. Run: sudo apt-get install pichost"
elif command -v dnf >/dev/null 2>&1; then
    sudo rpm --import "$REPO_BASE/public.key"
    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64) ARCHDIR="x86_64" ;;
        aarch64) ARCHDIR="aarch64" ;;
        *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
    esac
    sudo tee /etc/yum.repos.d/pichost.repo >/dev/null <<EOF
[pichost]
name=PicHost Repository
baseurl=$REPO_BASE/rpm/$ARCHDIR
enabled=1
gpgcheck=1
repo_gpgcheck=0
gpgkey=$REPO_BASE/public.key
EOF
    echo "Done. Run: sudo dnf install pichost"
else
    echo "Unsupported system (need apt-get or dnf)" >&2
    exit 1
fi
