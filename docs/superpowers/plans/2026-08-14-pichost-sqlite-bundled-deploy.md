# 内置 SQLite 优先部署(单目录安装)实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** SQLite 优先部署 — 数据库与存储落入软件目录 `data/` 子目录(单目录安装),install.sh 默认 sqlite 模式,uninstall 默认清除 + `--keep-data`,.env.example/文档同步为 SQLite 优先心智。

**Architecture:** 纯 bash 脚本 + 文档变更,零 Rust 代码。install.sh 删除 DATA_DIR 位置参数(单目录契约),`resolve_mode` 反转为 sqlite 优先,`generate_env` 将 DB URL 指向 `$INSTALL_DIR/data/pichost.db` 并双模式写入 storage 路径(sed 去重幂等);uninstall.sh 默认清除含 data/ 的 INSTALL_DIR(tty 确认 + `--keep-data`);verify-release.sh dry-run 改 2 参并断言默认 sqlite。设计文档:`docs/superpowers/specs/2026-08-14-pichost-sqlite-bundled-deploy-design.md`。

**Tech Stack:** bash(scripts/install.sh、uninstall.sh、verify-release.sh)、.env.example、README.md、AGENTS.md、CHANGELOG.md、Cargo.toml/package.json 版本号。

## Agent Worker Instructions

- **Required sub-skills**: `superpowers:subagent-driven-development`(推荐)或 `superpowers:executing-plans`;bash 脚本调试技能;无需 Rust 技能(本计划零 Rust 代码)
- **Execution mode**: `subagent-driven-development`(每任务新 subagent + 两段式评审)
- **Required verification**: `cargo test --workspace`(406 pass,无 infra)、`cargo clippy --workspace -- -D warnings`、`bash scripts/verify-release.sh --skip-test --skip-lint`(脚本冒烟)、`bash scripts/tests/*.sh`(本计划新增/修改的脚本测试)
- **Version bump reminder**: 0.21.0 → **0.22.0**(feature,minor)— Cargo.toml workspace + web-ui/package.json + Cargo.lock 对齐;提交信息英文语义化(break commit 用 `feat:`)

## Global Constraints

- **零 Rust 代码变更** — 应用层 `DatabaseMode::default()` 保持 postgres(Docker/CI 零变化);Rust 侧仅版本号
- install.sh 契约(breaking):`install.sh [--yes] [--mode postgres|sqlite] [INSTALL_DIR] [CONFIG_DIR]` — DATA_DIR 第三位置参数删除
- DB 文件:`$INSTALL_DIR/data/pichost.db`;storage:`$INSTALL_DIR/data/storage-local`(`PICHOST_STORAGE_LOCAL_BASE_PATH` 双模式显式写入,sed 去重幂等)
- 默认模式:无 `--mode` 时 tty 提问 `[1] SQLite(推荐) [2] PostgreSQL`;`--yes`/非 tty → sqlite;postgres 分支保留 apt 引导
- uninstall:默认 `rm -rf "$INSTALL_DIR"`(含 data/),tty 删除前确认;`--keep-data` 保留 data/;CONFIG_DIR 保留(提示手动清理)
- 脚本统一 `set -euo pipefail`;提交前运行 `bash -n` 语法检查
- 版本 0.22.0:Cargo.toml + web-ui/package.json + Cargo.lock 对齐;CHANGELOG Keep a Changelog 格式
- 不做:Docker compose 改动、应用层默认翻转、PG→SQLite 迁移工具、存量 storage 目录搬迁

## 任务依赖图

```
T0 (install.sh 单目录契约 + sqlite 默认) ──┬─→ T1 (uninstall.sh 契约 + --keep-data)
                                          ├─→ T2 (verify-release dry-run 2 参 + .env.example 翻转)
T3 (版本 0.22.0, 独立) ──→ T4 (README/AGENTS 文档同步) ──→ T5 (CHANGELOG/summary 收尾)
```

---

### Task T0: Change install.sh to single-dir contract with sqlite default

**Breaking:** true(位置参数契约删除 DATA_DIR + 默认模式翻转)

**Files:**
- Modify: `scripts/install.sh`
- Modify: `scripts/tests/install_test.sh`(测试文件,先行改写)

**Interfaces:**
- Produces: 新契约 `install.sh [--yes] [--mode postgres|sqlite] [INSTALL_DIR] [CONFIG_DIR]`;DB 路径 `$INSTALL_DIR/data/pichost.db`;`.env` 含 `PICHOST_STORAGE_LOCAL_BASE_PATH=$INSTALL_DIR/data/storage-local`;非 tty/`--yes` 默认 `MODE=sqlite`;`$INSTALL_DIR/data` 目录创建并 chown
- Consumes: 无(T0 为根任务)

- [ ] **Step 1: 重写 install_test.sh 为失败测试**

```bash
# scripts/tests/install_test.sh — 单目录契约 + sqlite 默认断言(重写)
#!/usr/bin/env bash
set -euo pipefail
# 用法: bash scripts/tests/install_test.sh <pkg_dir>
# 断言(0.22.0 单目录契约):
#   - 双位置参数 [INSTALL_DIR] [CONFIG_DIR];sqlite URL 指向 $INSTALL_DIR/data/pichost.db
#   - .env 含 PICHOST_STORAGE_LOCAL_BASE_PATH=$INSTALL_DIR/data/storage-local
#   - 无 --mode 时默认 sqlite(非 tty);重跑幂等(关键行无重复)
#   - 既有断言保留:JWT >=32 字符、.env 权限 600、单元无 postgresql 依赖
PKG_DIR="${1:?usage: install_test.sh <pkg_dir>}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# ① 显式 sqlite + 双位置参数(旧 3 参契约改为 2 参)
(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode sqlite "$TMP/pi" "$TMP/pc")

grep -q 'PICHOST_DATABASE_MODE=sqlite' "$TMP/pc/.env"
grep -Fq "sqlite://$TMP/pi/data/pichost.db" "$TMP/pc/.env"
grep -Fq "PICHOST_STORAGE_LOCAL_BASE_PATH=\"$TMP/pi/data/storage-local\"" "$TMP/pc/.env"
[ -d "$TMP/pi/data" ] || { echo "FAIL: data/ dir missing"; exit 1; }
! grep -q 'Wants=.*postgresql' "$TMP/pc/pichost-api.service"

# ② 默认模式(无 --mode,非 tty)→ sqlite
(cd "$PKG_DIR" && bash scripts/install.sh --yes "$TMP/pi2" "$TMP/pc2")
grep -q 'PICHOST_DATABASE_MODE=sqlite' "$TMP/pc2/.env"
grep -Fq "sqlite://$TMP/pi2/data/pichost.db" "$TMP/pc2/.env"

# ③ 幂等重跑:关键行各恰 1 次
(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode sqlite "$TMP/pi" "$TMP/pc")
[ "$(grep -c '^PICHOST_DATABASE_MODE=' "$TMP/pc/.env")" -eq 1 ] || { echo "FAIL: dup MODE line"; exit 1; }
[ "$(grep -c '^PICHOST_STORAGE_LOCAL_BASE_PATH=' "$TMP/pc/.env")" -eq 1 ] || { echo "FAIL: dup STORAGE line"; exit 1; }
[ "$(grep -c '^PICHOST_DATABASE_URL=' "$TMP/pc/.env")" -eq 1 ] || { echo "FAIL: dup URL line"; exit 1; }

# ④ postgres 模式:URL 必须被清理/替换,不得残留 sqlite URL(防止 .env.example 翻转后串配置)
(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode postgres "$TMP/pi3" "$TMP/pc3")
grep -q 'PICHOST_DATABASE_MODE=postgres' "$TMP/pc3/.env" \
  || { echo "FAIL: postgres mode not set"; exit 1; }
! grep -q '^PICHOST_DATABASE_URL=sqlite://' "$TMP/pc3/.env" \
  || { echo "FAIL: postgres mode left sqlite URL"; exit 1; }

# ⑤ 既有断言(保留)
jwt_line="$(grep -E '^PICHOST_AUTH(_|__)JWT_SECRET=' "$TMP/pc/.env" | tail -n 1)"
jwt_secret="${jwt_line#*=}"; jwt_secret="${jwt_secret%\"}"; jwt_secret="${jwt_secret#\"}"
[ "${#jwt_secret}" -ge 32 ] || { echo "FAIL: JWT too short"; exit 1; }
[ "$(stat -c '%a' "$TMP/pc/.env")" = "600" ] || { echo "FAIL: .env perms not 600"; exit 1; }
echo "install_test.sh PASS"
```

- [ ] **Step 2: 运行测试确认失败**

Run: `bash scripts/tests/install_test.sh <pkg_dir>`
Expected: FAIL — 当前 install.sh 仍接受 3 位置参数(第 3 个当 DATA_DIR),URL 指向 `$TMP/pd/pichost.db` 而非 `$TMP/pi/data/pichost.db`;无 STORAGE 行;`data/` 目录不存在

- [ ] **Step 3: 实现 install.sh 变更**

```bash
# ① 参数区:删除 DATA_DIR 默认值,位置参数收缩为 2 个
DATA_DIR=""
INSTALL_DIR="/opt/pichost"
CONFIG_DIR="/etc/pichost"
# 位置参数解析处:POS 2 → CONFIG_DIR;删除 POS 3 分支
# usage() 同步更新为 [INSTALL_DIR] [CONFIG_DIR]

# ② 头部注释同步:删除含 "/var/lib/pichost" 与 DATA_DIR 的说明行
#    (install.sh 第 5-8 行注释改写为:  install.sh [--yes] [--mode postgres|sqlite] [INSTALL_DIR] [CONFIG_DIR]
#      INSTALL_DIR 软件目录(默认 /opt/pichost);CONFIG_DIR 配置目录(默认 /etc/pichost);
#      DB/storage 位于 $INSTALL_DIR/data/ 下,SQLite 为默认推荐模式)

# ③ 新增派生变量(参数解析完成后)
DB_DIR="$INSTALL_DIR/data"

# ④ resolve_mode 反转:无 --mode 时 sqlite 优先
resolve_mode() {
    [ -n "$MODE" ] && return
    if [ "$ASSUME_YES" -eq 1 ] || ! is_tty; then
        MODE="sqlite"; return
    fi
    while true; do
        echo ">> 选择数据库模式:"
        echo "   [1] SQLite (推荐, 零外部依赖, 内嵌 worker)"
        echo "   [2] PostgreSQL (标准模式, 需 PG + Redis)"
        if ! read -r -p "请选择 [1/2]: " ans; then
            echo; echo ">> 输入已结束，退出"; exit 1
        fi
        case "$ans" in
            1) MODE="sqlite"; break ;;
            2) MODE="postgres"
               if ! has_pg; then
                   echo ">> PostgreSQL 未检测到"
                   echo "   [1] apt 自动安装 [2] 手动安装后重跑"
                   read -r -p "请选择 [1/2]: " p2 || { echo; exit 1; }
                   case "$p2" in
                       1) apt_install postgresql postgresql-client
                          command -v systemctl >/dev/null 2>&1 && \
                            systemctl enable --now postgresql >/dev/null 2>&1 || true ;;
                       *) echo ">> 请手动安装 PostgreSQL 后重新运行"; exit 1 ;;
                   esac
               fi
               break ;;
            *) echo ">> 无效选择: $ans" ;;
        esac
    done
}

# ⑤ generate_env:URL 指向 DB_DIR;双模式写入 storage 路径(sed 去重,同时清理
#    单/双下划线两种变体,防止 .env.example 拷贝后 figment 键冲突)
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
            printf '# SQLite 模式: 零外部依赖 (内嵌 worker, 默认推荐)\n'
            printf 'PICHOST_DATABASE_MODE=sqlite\n'
            printf 'PICHOST_DATABASE_URL="sqlite://%s/pichost.db"\n' "$DB_DIR"
        } >> "$env_file"
        echo ">> SQLite mode configured: $DB_DIR/pichost.db"
    else
        sed -i -e '/^PICHOST_DATABASE_MODE=/d' \
            -e '/^PICHOST_DATABASE_URL=/d' \
            -e '/^PICHOST_REDIS_URL=/d' "$env_file"
        {
            printf 'PICHOST_DATABASE_MODE=postgres\n'
            printf '# PICHOST_DATABASE_URL=postgresql://user:password@localhost:5432/pichost\n'
        } >> "$env_file"
        echo ">> PostgreSQL mode configured (edit $env_file for credentials)"
    fi
    # storage 路径双模式统一(去重后追加;单/双下划线变体都清理)
    sed -i -e '/^PICHOST_STORAGE_LOCAL_BASE_PATH=/d' \
        -e '/^PICHOST_STORAGE__LOCAL_BASE_PATH=/d' "$env_file"
    printf 'PICHOST_STORAGE_LOCAL_BASE_PATH="%s/storage-local"\n' "$DB_DIR" >> "$env_file"
    chmod 600 "$env_file"
}

# ⑥ 主流程:创建 data/ 目录(INSTALL_DIR 递归 chown 已覆盖)
mkdir -p "$INSTALL_DIR" "$CONFIG_DIR" "$DB_DIR"
```

- [ ] **Step 4: 运行测试确认通过**

Run: `bash scripts/tests/install_test.sh <pkg_dir>` 以及 `bash -n scripts/install.sh`
Expected: PASS;语法检查无输出

- [ ] **Step 5: 提交**

```bash
git add scripts/install.sh scripts/tests/install_test.sh
git commit -m "feat: single-dir install contract with sqlite default (data/ under INSTALL_DIR)"
```

**AC:**
- given: 双位置参数 `INSTALL_DIR`/`CONFIG_DIR` 的安装调用
- when: 执行 `install.sh --yes --mode sqlite <INSTALL_DIR> <CONFIG_DIR>`
- then: `.env` 含 `PICHOST_DATABASE_MODE=sqlite` 与 `sqlite://<INSTALL_DIR>/data/pichost.db`,且 `<INSTALL_DIR>/data` 目录存在
- given: 无 `--mode` 且非 tty 的安装调用
- when: 执行 `install.sh --yes <INSTALL_DIR> <CONFIG_DIR>`
- then: `.env` 含 `PICHOST_DATABASE_MODE=sqlite`(默认 sqlite)
- given: 同一 INSTALL_DIR 重跑安装
- when: 再次执行 install.sh
- then: `.env` 中 `PICHOST_DATABASE_MODE`/`PICHOST_DATABASE_URL`/`PICHOST_STORAGE_LOCAL_BASE_PATH` 各恰 1 行(幂等)

**regression:**
- `bash scripts/tests/verify_release_test.sh`(本任务未改动的既有脚本测试)
- `bash -n scripts/install.sh`(语法完整性)

**verify:**
- `bash scripts/tests/install_test.sh <pkg_dir>`
- `bash scripts/verify-release.sh --skip-test --skip-lint`(install dry-run 冒烟)
- `cargo clippy --workspace -- -D warnings`(N/A — 零 Rust 变更,基线确认)

---

### Task T1: Change uninstall.sh to wipe data/ by default with --keep-data

**Breaking:** true(卸载位置参数契约删除 DATA_DIR + 默认清除行为翻转)

**Files:**
- Modify: `scripts/uninstall.sh`
- Create: `scripts/tests/uninstall_test.sh`(测试文件,先行创建)

**Interfaces:**
- Produces: `uninstall.sh [--keep-data] [INSTALL_DIR] [CONFIG_DIR]`;默认 `rm -rf "$INSTALL_DIR"`(含 data/);`--keep-data` 保留 `$INSTALL_DIR/data`;CONFIG_DIR 默认保留并打印提示
- Consumes: T0 的 install.sh 双位置参数契约(测试内用它构造安装现场)

- [ ] **Step 1: 创建 uninstall_test.sh 失败测试**

```bash
# scripts/tests/uninstall_test.sh — uninstall.sh 单目录契约断言
#!/usr/bin/env bash
set -euo pipefail
# 用法: bash scripts/tests/uninstall_test.sh <pkg_dir>
# 断言:
#   - 默认(非 tty):INSTALL_DIR 整体删除(含 data/ 图片数据)
#   - --keep-data:INSTALL_DIR 保留且 data/ 存在,二进制已删除
#   - CONFIG_DIR 保留(默认策略)
PKG_DIR="${1:?usage: uninstall_test.sh <pkg_dir>}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# ① 安装现场(sqlite 单目录契约,来自 T0)
(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode sqlite "$TMP/pi" "$TMP/pc")
[ -d "$TMP/pi/data" ] || { echo "FAIL: setup install.sh (data/ missing)"; exit 1; }

# ② 默认卸载:整体清除(含 data/)
bash "$PKG_DIR/scripts/uninstall.sh" "$TMP/pi" "$TMP/pc"
[ ! -d "$TMP/pi" ] || { echo "FAIL: INSTALL_DIR not removed"; exit 1; }
[ -d "$TMP/pc" ] || { echo "FAIL: CONFIG_DIR should be preserved"; exit 1; }
echo "uninstall_test.sh: default wipe OK"

# ③ 重新安装 → --keep-data 卸载
(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode sqlite "$TMP/pi" "$TMP/pc")
bash "$PKG_DIR/scripts/uninstall.sh" --keep-data "$TMP/pi" "$TMP/pc"
[ -d "$TMP/pi/data" ] || { echo "FAIL: --keep-data lost data/"; exit 1; }
[ ! -f "$TMP/pi/pichost-api" ] || { echo "FAIL: binary still present"; exit 1; }
[ ! -f "$TMP/pi/web-ui/dist/index.html" ] || { echo "FAIL: static assets still present"; exit 1; }
echo "uninstall_test.sh: --keep-data OK"
echo "uninstall_test.sh PASS"
```

- [ ] **Step 2: 运行测试确认失败**

Run: `bash scripts/tests/uninstall_test.sh <pkg_dir>`
Expected: FAIL — 当前 uninstall.sh 位置参数第 2 个是 DATA_DIR(非 CONFIG_DIR),`rm -rf "$INSTALL_DIR"` 后数据落在 `$TMP/pd` 未被清除,断言 ② 的语义不匹配

- [ ] **Step 3: 实现 uninstall.sh 变更**

```bash
#!/bin/bash
set -euo pipefail

KEEP_DATA=0
INSTALL_DIR="/opt/pichost"
CONFIG_DIR="/etc/pichost"

usage() {
    echo "Usage: $0 [--keep-data] [INSTALL_DIR] [CONFIG_DIR]"
    echo "  --keep-data   preserve $INSTALL_DIR/data (images + SQLite DB)"
}

# --- 参数解析 ---
POS=0
while [ $# -gt 0 ]; do
    case "$1" in
        --keep-data) KEEP_DATA=1 ;;
        -h|--help) usage; exit 0 ;;
        -*) echo "unknown option: $1"; usage; exit 1 ;;
        *)
            POS=$((POS + 1))
            case "$POS" in
                1) INSTALL_DIR="$1" ;;
                2) CONFIG_DIR="$1" ;;
                *) echo "error: too many positional arguments: $1"; usage; exit 1 ;;
            esac ;;
    esac
    shift
done

echo "PicHost uninstalling..."

# 1. Stop and disable systemd services
if command -v systemctl &>/dev/null; then
    systemctl stop pichost-api pichost-worker 2>/dev/null || true
    systemctl disable pichost-api pichost-worker 2>/dev/null || true
    rm -f /etc/systemd/system/pichost-api.service
    rm -f /etc/systemd/system/pichost-worker.service
    systemctl daemon-reload
fi

# 2. data/ 处置:tty 且非 --keep-data 时确认
if [ "$KEEP_DATA" -eq 0 ] && [ -d "$INSTALL_DIR/data" ] && [ -t 0 ]; then
    read -r -p ">> $INSTALL_DIR/data 含全部图片与数据库,确认删除? [y/N] " ans || true
    [ "$ans" = "y" ] || [ "$ans" = "Y" ] || { echo ">> 取消;使用 --keep-data 保留数据"; exit 1; }
fi

# 3. Remove binaries and static files
if [ "$KEEP_DATA" -eq 1 ] && [ -d "$INSTALL_DIR/data" ]; then
    find "$INSTALL_DIR" -mindepth 1 ! -path "$INSTALL_DIR/data" ! -path "$INSTALL_DIR/data/*" -exec rm -rf {} +
    echo ">> Data preserved: $INSTALL_DIR/data (--keep-data)"
else
    rm -rf "$INSTALL_DIR"
fi

# 4. Config preserved
echo ">> Binaries removed"
echo ">> Config dir preserved: $CONFIG_DIR"
echo ">> To fully remove: rm -rf $CONFIG_DIR"
echo "PicHost uninstall complete."
```

- [ ] **Step 4: 运行测试确认通过**

Run: `bash scripts/tests/uninstall_test.sh <pkg_dir>` 以及 `bash -n scripts/uninstall.sh`
Expected: PASS;语法检查无输出

- [ ] **Step 5: 提交**

```bash
git add scripts/uninstall.sh scripts/tests/uninstall_test.sh
git commit -m "feat: uninstall wipes data/ by default with --keep-data escape hatch"
```

**AC:**
- given: 已安装(含 data/)且非 tty 的卸载调用
- when: 执行 `uninstall.sh <INSTALL_DIR> <CONFIG_DIR>`
- then: `<INSTALL_DIR>` 整体消失(含 data/),`<CONFIG_DIR>` 保留
- given: 已安装(含 data/)的卸载调用
- when: 执行 `uninstall.sh --keep-data <INSTALL_DIR> <CONFIG_DIR>`
- then: `<INSTALL_DIR>/data` 保留、二进制与静态资源已删除

**regression:**
- `bash scripts/tests/install_test.sh <pkg_dir>`(T0 测试须继续通过)
- `bash scripts/tests/verify_release_test.sh`

**verify:**
- `bash scripts/tests/uninstall_test.sh <pkg_dir>`
- `bash scripts/verify-release.sh --skip-test --skip-lint`
- `cargo clippy --workspace -- -D warnings`(N/A — 零 Rust 变更,基线确认)

---

### Task T2: Update verify-release.sh dry-run + flip .env.example defaults

**Breaking:** true(dry-run 调用契约变更 + .env.example 默认值翻转,影响手工/脚本消费者)

**Files:**
- Modify: `scripts/verify-release.sh`
- Modify: `.env.example`
- Modify: `scripts/tests/verify_release_test.sh`(测试文件,先行改写)

**Interfaces:**
- Produces: verify-release.sh 的 install dry-run 以双位置参数调用(容器内与本地两处);.env.example 默认 `PICHOST_DATABASE_MODE=sqlite`、`sqlite:///opt/pichost/data/pichost.db`、Redis 注释、storage 路径 `/opt/pichost/data/storage-local`;dry-run 后断言默认 sqlite
- Consumes: T0 的 install.sh 双位置参数契约

- [ ] **Step 1: 改写 verify_release_test.sh 为失败测试**

```bash
# scripts/tests/verify_release_test.sh — verify-release dry-run 契约 + .env.example 默认翻转断言
#!/usr/bin/env bash
set -euo pipefail
# 用法: bash scripts/tests/verify_release_test.sh
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# ① verify-release.sh:install dry-run 为双位置参数(无 DATA_DIR)
grep -q 'scripts/install.sh /opt/pichost /etc/pichost' "$ROOT/scripts/verify-release.sh" \
  || { echo "FAIL: verify-release.sh dry-run not 2-arg"; exit 1; }
# ② 全脚本目录不得残留旧 DATA_DIR 路径
! grep -rq '/var/lib/pichost' "$ROOT/scripts/" \
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `bash scripts/tests/verify_release_test.sh`
Expected: FAIL — 当前 dry-run 传 3 参、.env.example 默认 postgres、Redis 未注释、storage 路径 `./storage-local`

- [ ] **Step 3: 实现 verify-release.sh + .env.example**

```bash
# scripts/verify-release.sh — [7/7] 两处 dry-run 调用改双位置参数
# 容器内:
docker run --rm -v "$PKG_DIR:/$PKG_NAME:ro" ubuntu:24.04 \
    bash -c "cd /$PKG_NAME && bash scripts/install.sh /opt/pichost /etc/pichost"
# 本地回退:
(cd "$PKG_DIR" && bash scripts/install.sh /opt/pichost /etc/pichost)
# 并在 dry-run 后追加默认模式断言(容器内执行,失败即 exit 1):
#   test -d /opt/pichost/data && grep -q '^PICHOST_DATABASE_MODE=sqlite' /etc/pichost/.env \
#     && grep -q 'sqlite:///opt/pichost/data/pichost.db' /etc/pichost/.env
# 实现方式:在 dry-run bash -c 字符串中追加断言链(&& test ... && grep ...)
```

```bash
# .env.example — Database 段翻转(注意保留 DATABASE_URL 的 Docker 注释)
# Database (SQLite 为默认推荐模式;PostgreSQL 为备选标准模式)
PICHOST_DATABASE_MODE=sqlite
PICHOST_DATABASE_URL=sqlite:///opt/pichost/data/pichost.db
# 备选: PostgreSQL 标准模式(需 PG + Redis, 独立 worker 进程)
# PICHOST_DATABASE_MODE=postgres
# PICHOST_DATABASE_URL=postgresql://user:password@localhost:5432/pichost

# Redis (仅 postgres 模式需要)
# PICHOST_REDIS_URL=redis://localhost:6379

# Storage (local) — 单目录安装:数据位于软件目录 data/ 下
PICHOST_STORAGE__LOCAL_BASE_PATH=/opt/pichost/data/storage-local
```

- [ ] **Step 4: 运行测试确认通过**

Run: `bash scripts/tests/verify_release_test.sh` 以及 `bash -n scripts/verify-release.sh`
Expected: PASS;语法检查无输出

- [ ] **Step 5: 提交**

```bash
git add scripts/verify-release.sh .env.example scripts/tests/verify_release_test.sh
git commit -m "feat: 2-arg install dry-run with sqlite default assertion; .env.example sqlite-first"
```

**AC:**
- given: 全 `scripts/` 目录
- when: 搜索 `/var/lib/pichost`
- then: 无任何残留(单目录契约完全替换)
- given: `.env.example`
- when: 读取 Database 段
- then: `PICHOST_DATABASE_MODE=sqlite` 未注释、URL 为 `sqlite:///opt/pichost/data/pichost.db`、`PICHOST_REDIS_URL` 注释且注明仅 postgres 模式
- given: verify-release.sh 的 install dry-run
- when: 执行 `--skip-test --skip-lint`
- then: dry-run 以双位置参数调用 install.sh,并断言 .env 默认 sqlite + data/ URL

**regression:**
- `bash scripts/tests/install_test.sh <pkg_dir>`(install.sh 行为不回归)
- `bash scripts/tests/uninstall_test.sh <pkg_dir>`(T1 测试须继续通过)

**verify:**
- `bash scripts/tests/verify_release_test.sh`
- `bash scripts/verify-release.sh --skip-test --skip-lint`
- `cargo clippy --workspace -- -D warnings`(N/A — 零 Rust 变更,基线确认)

---

### Task T3: Bump version to 0.22.0 with real version alignment test

**Breaking:** false(版本号变更,无行为契约变化)

**Files:**
- Modify: `Cargo.toml`(workspace `version`)
- Modify: `web-ui/package.json`(`version`)
- Modify: `scripts/tests/version_check_test.sh`(测试文件,先行改写 — 原为占位符)

**Interfaces:**
- Produces: 全仓版本 0.22.0(Cargo.toml + web-ui/package.json + Cargo.lock/package-lock.json 对齐)
- Consumes: 无

- [ ] **Step 1: 改写 version_check_test.sh 为失败测试**

```bash
# scripts/tests/version_check_test.sh — 版本对齐断言(0.22.0)
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
grep -q '^version = "0.22.0"' "$ROOT/Cargo.toml" \
  || { echo "FAIL: Cargo.toml not 0.22.0"; exit 1; }
grep -q '"version": "0.22.0"' "$ROOT/web-ui/package.json" \
  || { echo "FAIL: package.json not 0.22.0"; exit 1; }
grep -q '"version": "0.22.0"' "$ROOT/web-ui/package-lock.json" \
  || { echo "FAIL: package-lock.json not 0.22.0"; exit 1; }
grep -A1 'name = "pichost-api"' "$ROOT/Cargo.lock" | grep -q '0.22.0' \
  || { echo "FAIL: Cargo.lock pichost-api not 0.22.0"; exit 1; }
echo "version_check_test.sh PASS"
```

- [ ] **Step 2: 运行测试确认失败**

Run: `bash scripts/tests/version_check_test.sh`
Expected: FAIL — 当前均为 0.21.0

- [ ] **Step 3: 实现版本 bump**

```bash
sed -i 's/^version = "0.21.0"/version = "0.22.0"/' Cargo.toml
sed -i 's/"version": "0.21.0"/"version": "0.22.0"/' web-ui/package.json web-ui/package-lock.json
# 重新生成 Cargo.lock(workspace 成员版本同步)
cargo check --workspace
# 确认 Cargo.lock 内 pichost-* 包版本为 0.22.0
grep -A1 'name = "pichost-api"' Cargo.lock | grep -q '0.22.0'
```

- [ ] **Step 4: 运行测试确认通过**

Run: `bash scripts/tests/version_check_test.sh`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add Cargo.toml Cargo.lock web-ui/package.json web-ui/package-lock.json scripts/tests/version_check_test.sh
git commit -m "chore: bump version to 0.22.0 (sqlite-first deployment feature)"
```

**AC:**
- given: workspace 根 `Cargo.toml` 与 `web-ui/package.json`
- when: 执行 `bash scripts/tests/version_check_test.sh`
- then: 全部断言通过(三处版本均 0.22.0)
- given: Cargo.lock
- when: `cargo check --workspace` 后检索 `name = "pichost-api"`
- then: 版本为 0.22.0

**regression:**
- `cargo check --workspace`(版本变更不破坏编译)

**verify:**
- `bash scripts/tests/version_check_test.sh`
- `cargo test --workspace`(406 pass 基线)
- `cargo clippy --workspace -- -D warnings`(N/A — 仅版本号,基线确认)

---

### Task T4: Sync README.md and AGENTS.md to sqlite-first single-dir deployment

**Breaking:** false(纯文档)

**Files:**
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `scripts/tests/docs_check_test.sh`(测试文件,先行扩展)

**Interfaces:**
- Consumes: T0–T3 的契约(install.sh 签名、data/ 路径、默认模式、版本 0.22.0)
- Produces: README/AGENTS 反映 SQLite 优先部署(单目录、默认 sqlite、卸载行为、版本 0.22.0)

- [ ] **Step 1: 扩展 docs_check_test.sh 为失败测试**

```bash
# scripts/tests/docs_check_test.sh — 追加 0.22.0 部署文档断言(保留既有 grep)
# 既有断言保留:PICHOST_DATABASE_MODE in README / sqlite in AGENTS / 轻量模式 in summary
# 新增:
grep -q 'data/pichost.db' "$ROOT/README.md" \
  || { echo "FAIL: README missing data/ sqlite path"; exit 1; }
grep -q '\[INSTALL_DIR\] \[CONFIG_DIR\]' "$ROOT/AGENTS.md" \
  || { echo "FAIL: AGENTS missing new install.sh signature"; exit 1; }
grep -q '0.22.0' "$ROOT/AGENTS.md" \
  || { echo "FAIL: AGENTS version not 0.22.0"; exit 1; }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `bash scripts/tests/docs_check_test.sh`
Expected: FAIL — README 无 data/ 路径、AGENTS 为旧签名与 0.21.0

- [ ] **Step 3: 实现文档同步**

README.md 变更点(Deployment/systemd 小节 + 版本标语 + Production checklist):
- 版本标语行 → `**v0.22.0** — SQLite-first deployment (single-directory install: DB + storage under <INSTALL_DIR>/data, sqlite default mode)`
- systemd 小节:install.sh 用法改为双位置参数;默认 sqlite;`sqlite:///opt/pichost/data/pichost.db`;storage 落 `data/storage-local`;uninstall 默认清除 data/、`--keep-data` 保留;Production checklist 增加 "备份 `/opt/pichost/data/`(含 pichost.db 与图片)"

AGENTS.md 变更点:
- Version 行 `0.21.0` → `0.22.0`,标语追加 SQLite-first 描述
- Setup Gotchas / Deployment 段落:install.sh 签名 `[INSTALL_DIR] [CONFIG_DIR]` + `--keep-data`;默认 sqlite;DB/storage 路径 `data/`
- 关键命令表:`bash scripts/verify-release.sh` 注释补充 sqlite 默认断言

- [ ] **Step 4: 运行测试确认通过**

Run: `bash scripts/tests/docs_check_test.sh`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add README.md AGENTS.md scripts/tests/docs_check_test.sh
git commit -m "docs: sqlite-first single-dir deployment in README/AGENTS"
```

**AC:**
- given: README.md
- when: 检索 `data/pichost.db` 与 sqlite 默认模式描述
- then: 存在(单目录部署已文档化)
- given: AGENTS.md
- when: 检索 `[INSTALL_DIR] [CONFIG_DIR]` 与 `0.22.0`
- then: 存在(新契约与版本已同步)

**regression:**
- `bash scripts/tests/version_check_test.sh`

**verify:**
- `bash scripts/tests/docs_check_test.sh`
- `bash scripts/tests/install_test.sh <pkg_dir>`(脚本未回归)
- `cargo clippy --workspace -- -D warnings`(N/A — 零 Rust 变更,基线确认)

---

### Task T5: Finalize CHANGELOG and summary for 0.22.0

**Breaking:** false(纯文档)

**Files:**
- Modify: `CHANGELOG.md`
- Modify: `.omo/summary/summary_and_next.md`
- Modify: `scripts/tests/docs_check_test.sh`(测试文件,先行扩展)

**Interfaces:**
- Consumes: T4 文档(README/AGENTS);T3 版本 0.22.0
- Produces: CHANGELOG 0.22.0 条目;summary 新阶段小节

- [ ] **Step 1: 扩展 docs_check_test.sh 为失败测试**

```bash
# scripts/tests/docs_check_test.sh — 追加 0.22.0 收尾断言
grep -q '0.22.0' "$ROOT/CHANGELOG.md" \
  || { echo "FAIL: CHANGELOG missing 0.22.0"; exit 1; }
grep -q 'SQLite 优先' "$ROOT/.omo/summary/summary_and_next.md" \
  || { echo "FAIL: summary missing sqlite-first section"; exit 1; }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `bash scripts/tests/docs_check_test.sh`
Expected: FAIL — CHANGELOG 无 0.22.0、summary 无新阶段

- [ ] **Step 3: 实现收尾文档**

CHANGELOG.md(Keep a Changelog 顶部插入):
```markdown
## [0.22.0] - 2026-08-14

### Added
- SQLite-first bare-metal deployment: single-directory install (`<INSTALL_DIR>/data/pichost.db` + `data/storage-local`), `install.sh` defaults to sqlite mode (interactive menu: SQLite recommended / PostgreSQL opt-in)
- `uninstall.sh --keep-data` to preserve image data on uninstall (default wipes `INSTALL_DIR` including `data/`)

### Changed
- `install.sh` positional contract: `[INSTALL_DIR] [CONFIG_DIR]` (DATA_DIR arg removed)
- `.env.example` defaults to sqlite (`PICHOST_DATABASE_MODE=sqlite`, `sqlite:///opt/pichost/data/pichost.db`)
- `verify-release.sh` install dry-run uses the 2-arg contract and asserts sqlite default
```

`.omo/summary/summary_and_next.md` — 顶部新增小节:
```markdown
## 内置 SQLite 优先部署(单目录安装)✅ (本次完成)

- **单目录契约**: install.sh 删除 DATA_DIR 位置参数,DB 与 storage 落入 `$INSTALL_DIR/data/`(`data/pichost.db` + `data/storage-local`),安装/卸载/权限管理单一目录
- **默认模式反转**: install.sh 无 --mode 时默认 sqlite(交互菜单 SQLite 推荐);`.env.example` 默认 sqlite;应用层 DatabaseMode 默认保持 postgres(Docker/CI 零变化)
- **卸载策略**: uninstall.sh 默认清除含 data/ 的 INSTALL_DIR(tty 确认),`--keep-data` 保留数据
- **verify-release.sh**: dry-run 双位置参数 + 默认 sqlite 断言
- **验证**: install_test/uninstall_test/verify_release_test/version_check_test/docs_check_test 全 PASS;`cargo test --workspace` 零回归(无 Rust 变更)
- **版本**: 0.21.0 → 0.22.0
```

- [ ] **Step 4: 运行测试确认通过 + 全量验证**

Run: `bash scripts/tests/docs_check_test.sh`
Expected: PASS

- [ ] **Step 5: 全量验证 + 提交**

```bash
bash scripts/tests/install_test.sh <pkg_dir>
bash scripts/tests/uninstall_test.sh <pkg_dir>
bash scripts/tests/verify_release_test.sh
bash scripts/tests/version_check_test.sh
cargo test --workspace
cargo clippy --workspace -- -D warnings
git add CHANGELOG.md .omo/summary/summary_and_next.md scripts/tests/docs_check_test.sh
git commit -m "docs: changelog + summary for 0.22.0 sqlite-first deployment"
```

**AC:**
- given: CHANGELOG.md
- when: 检索 `0.22.0`
- then: 存在 Keep a Changelog 格式条目(Added/Changed 分组)
- given: `.omo/summary/summary_and_next.md`
- when: 检索 `SQLite 优先`
- then: 存在本次完成小节(含验证与版本)

**regression:**
- `bash scripts/tests/docs_check_test.sh`(T4 扩展的既有断言)

**verify:**
- `bash scripts/tests/*.sh` 全量
- `cargo test --workspace` + `cargo clippy --workspace -- -D warnings`
- `bash scripts/verify-release.sh --skip-test --skip-lint`(可选,完整发布冒烟)
