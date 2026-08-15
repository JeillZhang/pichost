#!/bin/bash
# packaging/rpm/preuninstall.sh — rpm %preun ($1=0 卸载 / 1 升级)
set -e
if [ "$1" = "0" ]; then
    systemctl stop pichost-api pichost-worker 2>/dev/null || true
    rm -f /usr/lib/systemd/system/pichost-api.service /usr/lib/systemd/system/pichost-worker.service
    systemctl daemon-reload 2>/dev/null || true
fi
exit 0
