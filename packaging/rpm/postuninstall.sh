#!/bin/bash
# packaging/rpm/postuninstall.sh — rpm %postun (仅卸载时清数据)
set -e
if [ "$1" = "0" ]; then
    rm -rf /var/lib/pichost
    rm -f /etc/pichost/.env
fi
exit 0
