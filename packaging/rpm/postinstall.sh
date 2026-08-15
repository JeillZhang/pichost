#!/bin/bash
# packaging/rpm/postinstall.sh — rpm %post (复用共享 lib)
set -e
# 建用户/建目录/生成 .env + JWT/启动服务
source /usr/share/pichost/install-lib.sh
ensure_pkg_user
ensure_pkg_dirs
generate_pkg_env
ensure_pkg_jwt
set_pkg_ownership
enable_pkg_services
exit 0
