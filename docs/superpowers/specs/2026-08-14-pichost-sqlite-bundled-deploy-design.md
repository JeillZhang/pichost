# PicHost 内置 SQLite 优先部署(单目录安装)— 设计文档

> **日期**: 2026-08-14
> **目标**: SQLite 已内置(bundled,0.21.0 轻量模式)。本期将部署策略改为"SQLite 优先":数据库文件与存储目录落入 pichost 软件目录内的 `data/` 子目录(单目录安装),裸机安装默认使用 SQLite 模式;PostgreSQL 变为显式选择项
> **范围**: 纯脚本 + 文档变更(install.sh / uninstall.sh / verify-release.sh / .env.example / README / AGENTS.md / CHANGELOG / summary)。**零 Rust 代码变更**
> **版本**: 0.21.1 → 0.22.0(feature)
> **前置**: `docs/superpowers/specs/2026-08-09-pichost-sqlite-mode-design.md`(轻量模式已落地)

---

## 1. 背景与目标

### 1.1 现状

0.21.0 已实现 SQLite 轻量模式(bundled sqlite、单进程内嵌 worker、零外部依赖),但部署策略仍偏向 PostgreSQL:

| 现状 | 问题 |
|------|------|
| 安装目录分离:`INSTALL_DIR=/opt/pichost` + `DATA_DIR=/var/lib/pichost` | DB 文件与存储散落两处,卸载/迁移/权限管理复杂 |
| `resolve_mode` 自动检测:检测到 `pg_isready` 即选 postgres | 有 PG 的机器默认进重模式;SQLite 仅在无 PG 时兜底 |
| 交互菜单顺序:先推荐 apt 安装 PG,SQLite 为"改用"选项 | SQLite 未被定位为首选 |
| `.env.example` 默认展示 postgres 连接串 | 文档心智模型仍是 PG 优先 |
| sqlite 模式不写 `PICHOST_STORAGE_LOCAL_BASE_PATH` | 存储落在 `./storage-local`(相对 WorkingDirectory),与 DB 分离 |

### 1.2 已确认决策(brainstorming 澄清)

| # | 决策点 | 结论 |
|---|--------|------|
| D1 | DB 文件位置 | `$INSTALL_DIR/data/pichost.db` — 软件目录内 `data/` 子目录,同时存放 storage-local;单目录安装、卸载即清 |
| D2 | "优先使用"范围 | **安装/文档层默认 sqlite**;应用层 `config.rs` 的 `DatabaseMode::default()` 保持 postgres(Docker/CI 显式设 env,零变化) |
| D3 | 卸载数据处置 | uninstall.sh 默认清除整个 INSTALL_DIR(含 data/ 图片数据),tty 时删除前确认;`--keep-data` 保留数据 |
| D4 | 版本 | 0.22.0(feature,minor 递增) |

### 1.3 成功标准

1. `install.sh` 无参数(或 `--yes`)在无 PG 机器上直接完成 SQLite 单目录安装,`.env` 指向 `sqlite://$INSTALL_DIR/data/pichost.db`
2. 交互菜单 SQLite 为推荐项(`[1] SQLite(推荐) / [2] PostgreSQL`);选 postgres 且缺依赖时保留现有 apt 引导
3. 两种模式下 `PICHOST_STORAGE_LOCAL_BASE_PATH` 均写入 `$INSTALL_DIR/data/storage-local`(sed 去重,重跑幂等)
4. `uninstall.sh` 默认清除含 data/ 的 INSTALL_DIR,tty 确认 + `--keep-data` 逃生
5. `verify-release.sh` dry-run 断言默认模式 = sqlite 且 URL 指向软件目录
6. `.env.example` / README / AGENTS.md 同步为 SQLite 优先心智;`cargo test --workspace` + clippy 零回归(无 Rust 变更)

---

## 2. 目标部署形态

```mermaid
flowchart TB
    subgraph INSTALL["/opt/pichost (INSTALL_DIR, 单目录)"]
        BIN["pichost-api / pichost-worker<br/>(bundled SQLite 单二进制)"]
        WEB["web-ui/dist 静态资源"]
        MIG["migrations/ + migrations-sqlite/"]
        DATA["data/ (pichost:pichost)<br/>├── pichost.db (SQLite, WAL)<br/>└── storage-local/ (图片文件)"]
    end

    subgraph CONFIG["/etc/pichost (CONFIG_DIR)"]
        ENV[".env — PICHOST_DATABASE_MODE=sqlite<br/>PICHOST_DATABASE_URL=sqlite:///opt/pichost/data/pichost.db<br/>PICHOST_STORAGE_LOCAL_BASE_PATH=/opt/pichost/data/storage-local"]
        UNIT["pichost-api.service (systemd)"]
    end

    BIN --> ENV
    ENV --> DATA
    UNIT --> BIN
```

**与现状(0.21.0)对比**:

| 项 | 现状 | 目标 |
|----|------|------|
| 目录 | INSTALL_DIR + DATA_DIR(/var/lib/pichost) 分离 | 仅 INSTALL_DIR,数据入 `data/` 子目录 |
| 默认模式 | 检测到 PG → postgres;否则 sqlite | **sqlite(无条件优先)**;postgres 显式选择 |
| 交互菜单 | `[1] 装 PG [2] 改 SQLite [3] 手动装` | `[1] SQLite(推荐) [2] PostgreSQL` |
| sqlite URL | `sqlite:///var/lib/pichost/pichost.db` | `sqlite:///opt/pichost/data/pichost.db` |
| storage 路径 | 默认 `./storage-local`(相对 WorkingDirectory) | `$INSTALL_DIR/data/storage-local`(显式写入 .env,双模式一致) |
| uninstall | 保留 DATA_DIR | 默认清除 data/,`--keep-data` 保留 |

---

## 3. 变更清单

### 3.1 `scripts/install.sh`(核心)

**参数契约变更(breaking,0.21.0 引入无存量负担)**:

```
install.sh [--yes] [--mode postgres|sqlite] [INSTALL_DIR] [CONFIG_DIR]
  --yes                  无人值守(--mode 缺省 → sqlite)
  --mode postgres|sqlite 强制指定模式(缺省: tty 提问 / 非 tty → sqlite)
  INSTALL_DIR            软件目录(默认 /opt/pichost)
  CONFIG_DIR             配置目录(默认 /etc/pichost)
```

- 删除 `DATA_DIR` 位置参数与默认值 `/var/lib/pichost`
- 新增派生变量:`DB_DIR="$INSTALL_DIR/data"`

**`resolve_mode` 反转**:

```mermaid
flowchart TD
    A["--mode 已指定?"] -->|是| B["使用指定模式"]
    A -->|否| C{"is_tty?"}
    C -->|是| D["提问: [1] SQLite(推荐,零依赖) [2] PostgreSQL"]
    C -->|否| E["默认 sqlite"]
    D -->|1| F["MODE=sqlite"]
    D -->|2| G["检查 pg_isready → 缺失则 apt 引导(现有逻辑保留)"]
    E --> F
```

**`generate_env` 变更**:
- sqlite 分支:`PICHOST_DATABASE_URL="sqlite://$DB_DIR/pichost.db"`(原 `$DATA_DIR/pichost.db`)
- 双模式统一追加(带 sed 去重,保证重跑幂等):
  `PICHOST_STORAGE_LOCAL_BASE_PATH="$DB_DIR/storage-local"`
- postgres 分支不变(仅 MODE 覆写 + 提示编辑凭据)

**目录与权限**:`mkdir -p "$INSTALL_DIR" "$CONFIG_DIR" "$DB_DIR"`;`chown pichost:pichost` 覆盖 INSTALL_DIR/CONFIG_DIR(DB_DIR 随 INSTALL_DIR 递归)

### 3.2 `scripts/uninstall.sh`

```
uninstall.sh [--keep-data] [INSTALL_DIR] [CONFIG_DIR]
```

- 删除 DATA_DIR 位置参数;新增 `--keep-data` 标志
- 默认:`rm -rf "$INSTALL_DIR"`(含 data/);tty 时先提示"将删除 data/ 下全部图片数据,确认?"
- `--keep-data`:`rm -rf` 排除 `$INSTALL_DIR/data`,打印保留提示
- CONFIG_DIR(/etc/pichost .env + 单元)保留策略不变(默认保留,提示手动清理)

### 3.3 `scripts/verify-release.sh`

- install dry-run 调用改传 2 参:容器内 `bash scripts/install.sh /opt/pichost /etc/pichost`
- dry-run 后新增断言(默认模式翻转回归保护):
  - `$INSTALL_DIR/.env` 含 `PICHOST_DATABASE_MODE=sqlite`
  - `$INSTALL_DIR/.env` 含 `sqlite:///opt/pichost/data/pichost.db`
  - `$INSTALL_DIR/data` 目录存在
- 布局检查列表不变(install.sh 仍随包发布)

### 3.4 `.env.example`

| 行 | 现状 | 目标 |
|----|------|------|
| `PICHOST_DATABASE_MODE` | 注释 `# PICHOST_DATABASE_MODE=postgres` | 默认 `PICHOST_DATABASE_MODE=sqlite`(注释说明 postgres 为备选) |
| `PICHOST_DATABASE_URL` | `postgresql://user:password@...` | `sqlite:///opt/pichost/data/pichost.db`(postgres 串留注释示例) |
| `PICHOST_REDIS_URL` | 默认启用 | 注释并注明"仅 postgres 模式" |
| `PICHOST_STORAGE__LOCAL_BASE_PATH` | `./storage-local` | `/opt/pichost/data/storage-local` |
| `DATABASE_URL`(sqlx 助手) | 保留 | 保留不动(Docker 专用,与应用无关) |

### 3.5 systemd 单元

- `scripts/pichost-api.service`:**无改动** — `WorkingDirectory=/opt/pichost` 已就绪;DB/storage 路径经 `.env` 绝对路径注入
- `scripts/pichost-worker.service`:不变(仅 postgres 模式安装)

### 3.6 文档同步

| 文件 | 变更 |
|------|------|
| README.md | 版本标语;Deployment/systemd 小节改为 SQLite 优先 + 单目录说明(data/ 位置、默认清除);Production checklist 补充 data/ 备份提醒 |
| AGENTS.md | install.sh 签名(`[INSTALL_DIR] [CONFIG_DIR]` + `--keep-data`)、默认模式、版本 0.22.0、目录结构 |
| CHANGELOG.md | 0.22.0 条目(Keep a Changelog) |
| `.omo/summary/summary_and_next.md` | 新阶段小节 + 待实施表清理 |
| 本设计文档 | 提交入库 |

---

## 4. 边界与已知限制

**本期不做**:
- 应用层默认翻转(`DatabaseMode::default()` 保持 Postgres — D2)
- Docker compose 改动(标准模式容器化不动)
- PG→SQLite 数据迁移工具
- 存量安装的 storage 目录迁移:重跑 install.sh 后新图写入 data/,旧图留在原地(README 注明;不做自动搬迁)

**升级注意事项**:已部署 postgres 模式的机器重跑 install.sh 时,`generate_env` 的 sed 去重确保不会产生重复 `PICHOST_STORAGE_LOCAL_BASE_PATH` 行;新增行只影响新上传图片的落盘位置。

---

## 5. 测试计划

| 测试 | 内容 | 门控 |
|------|------|------|
| install.sh dry-run(sqlite 默认) | verify-release.sh 容器内安装 → 断言 .env 三行 + data/ 存在 | 发布前 |
| install.sh `--mode postgres` 分支 | dry-run 覆盖显式 postgres(依赖检测走 WARNING 路径) | 发布前 |
| uninstall.sh `--keep-data` | 容器内验证 data/ 保留、其余清除 | 发布前 |
| 幂等重跑 | 同一 INSTALL_DIR 重跑 install.sh,`.env` 无重复行 | 发布前 |
| Rust 回归 | `cargo test --workspace`(406 pass)+ clippy 零警告 — 无代码变更,仅确认 | 必须 |
| 前端 | `npm run build` 不受影响(不涉及 web-ui) | 不涉及 |

---

## 6. 实施顺序

| 阶段 | 内容 | 依赖 |
|------|------|------|
| S1 | install.sh:参数契约 + resolve_mode 反转 + generate_env 重写 + data/ 目录 | 无 |
| S2 | uninstall.sh:参数契约 + 默认清除 + `--keep-data` | S1 |
| S3 | verify-release.sh:dry-run 2 参 + sqlite 默认断言 | S1 |
| S4 | .env.example 默认值翻转 | 无 |
| S5 | 文档:README / AGENTS.md / CHANGELOG / summary + 版本 0.22.0 | S1–S4 |
| S6 | 验证:verify-release.sh 全流程 + cargo test + clippy | S1–S5 |

---

## 7. TODO 跟踪

- [x] S1 install.sh 改造
- [x] S2 uninstall.sh 改造
- [x] S3 verify-release.sh 适配
- [x] S4 .env.example 默认值
- [x] S5 文档同步 + 版本 0.22.0
- [x] S6 全量验证
