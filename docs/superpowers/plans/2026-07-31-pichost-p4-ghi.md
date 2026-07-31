# PicHost P4-G/H/I Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement P4-G (Settings UI optimization with user dropdown + accordion layout), P4-H (software packaging with install/uninstall scripts, systemd services, and GitHub Actions release CI), and P4-I (admin system configuration management UI that writes config.toml).

**Architecture:** Three independent phases. P4-G is pure frontend (no backend changes). P4-H is DevOps (shell scripts + CI workflow). P4-I adds a backend config service (toml_edit read/write) + 6 admin API endpoints + a frontend tab. No new database migrations required — P4-I writes to config.toml on disk, not to DB.

**Tech Stack:** React 19 + TypeScript 7 + Tailwind CSS 4 (frontend), Rust + Axum (backend config service), Bash (install scripts), GitHub Actions (CI/CD).

## Global Constraints

- Rust functions ≤50 lines, lines ≤120 chars
- `cargo clippy --workspace -- -D warnings` — zero warnings required
- `cargo test --workspace` — all existing tests must keep passing (68 pass, 10 ignored baseline)
- `npm run build` (tsc -b && vite build) — frontend must build clean
- Version bumps: P4-G → v0.16.4, P4-H → v0.17.0, P4-I → v0.17.1
- All config env vars use `PICHOST_` prefix
- Frontend follows existing CSS variable pattern (`var(--color-*)`), NOT Tailwind default palette
- Admin endpoints: JWT + Admin role required

## Agent Worker Instructions

- **Required sub-skills**: `superpowers:subagent-driven-development` (preferred) or `superpowers:executing-plans`
- **Verification gates** (after each phase):
  - P4-G: `npm run build` (tsc + vite)
  - P4-H: `bash -n scripts/*.sh` (shell syntax check) + `cargo test --workspace`
  - P4-I: `cargo clippy --workspace -- -D warnings` + `cargo test --workspace` + `npm run build`
- **Version bump**: update version in `Cargo.toml` (workspace) + `web-ui/package.json` after each phase
- After P4-I completes: auto-sync `AGENTS.md`, `README.md`, `.omo/summary/summary_and_next.md`

---

### Task T0: Create DropdownMenu UI component (P4-G)

**Files:**
- Create: `web-ui/src/components/ui/DropdownMenu.tsx`
- Verify: `npm run build` (project has no frontend unit-test infrastructure per AGENTS.md)

**Depends on:** [] (P4-G Phase 1 — independent)

**Breaking:** false

**Acceptance Criteria:**
- given: DropdownMenu renders with trigger element and menu items
- when: user clicks trigger
- then: menu appears with correct items, clicking outside closes it, Escape closes it
- given: menu is open
- when: user clicks a menu item
- then: onClick fires and menu closes

**Regression:**
- `npm run build` (all existing components must still compile)

**Implementation:**

```tsx
// test_code: Verify via manual browser testing and `npm run build`
// No unit test infrastructure for components exists in this project

// impl_code:
// web-ui/src/components/ui/DropdownMenu.tsx
import { useState, useRef, useEffect, type ReactNode } from 'react';

export interface DropdownMenuItem {
  label: string;
  icon?: ReactNode;
  onClick: () => void;
  danger?: boolean;
}

interface DropdownMenuProps {
  trigger: ReactNode;
  items: DropdownMenuItem[];
  align?: 'left' | 'right';
}

export default function DropdownMenu({ trigger, items, align = 'right' }: DropdownMenuProps) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('click', handleClick);
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('click', handleClick);
      document.removeEventListener('keydown', handleKey);
    };
  }, [open]);

  const handleItemClick = (item: DropdownMenuItem) => {
    item.onClick();
    setOpen(false);
  };

  const handleTriggerClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setOpen((prev) => !prev);
  };

  return (
    <div className="relative inline-block">
      <button
        ref={triggerRef}
        type="button"
        onClick={handleTriggerClick}
        className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm
                   text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]
                   hover:bg-[var(--color-surface-hover)] transition-colors"
      >
        {trigger}
      </button>
      {open && (
        <div
          ref={menuRef}
          className={`absolute z-50 mt-1 min-w-[180px] rounded-lg border
                      border-[var(--color-border)] bg-[var(--color-surface-elevated)]
                      py-1 shadow-lg backdrop-blur-md ${align === 'right' ? 'right-0' : 'left-0'}`}
        >
          {items.map((item, i) => (
            <button
              key={i}
              type="button"
              onClick={() => handleItemClick(item)}
              className={`flex w-full items-center gap-2 px-3 py-2 text-sm
                          transition-colors ${
                            item.danger
                              ? 'text-red-400 hover:bg-red-500/10'
                              : 'text-[var(--color-text-primary)] hover:bg-[var(--color-surface-hover)]'
                          }`}
            >
              {item.icon && <span className="w-4 h-4 flex-shrink-0">{item.icon}</span>}
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
```

**Verification:**
- `npm run build` (TypeScript compiles cleanly)

---

### Task T1: Add user dropdown to NavBar (P4-G)

**Files:**
- Modify: `web-ui/src/components/NavBar.tsx` (replace lines 74-108 user section)

**Depends on:** [T0] (DropdownMenu component)

**Breaking:** false

**Acceptance Criteria:**
- given: user is logged in (regular user)
- when: viewing NavBar
- then: avatar/username dropdown shows Settings, Theme toggle, and Logout items
- given: user is admin
- when: viewing NavBar
- then: dropdown shows Settings, Admin, Theme toggle, and Logout items
- given: user clicks Settings
- when: in dropdown
- then: navigates to /settings
- given: user clicks Logout
- when: in dropdown
- then: calls logout() and navigates to /login

**Regression:**
- `npm run build` (NavBar must compile)
- Existing nav links (Dashboard, Gallery, Admin) must still render and navigate correctly

**Implementation:**

```tsx
// test_code: Manual browser testing + `npm run build`

// impl_code: Modify web-ui/src/components/NavBar.tsx
// REPLACE lines 74-108 (user section) with:

import { Settings, Shield, SunMoon, LogOut, User } from 'lucide-react';
import DropdownMenu from './ui/DropdownMenu';
import type { DropdownMenuItem } from './ui/DropdownMenu';

// Inside NavBar component, replace the user section:
// REPLACE: <div className="flex items-center gap-3"> ... (lines 74-108)
// WITH:
<div className="flex items-center gap-3">
  <ThemeToggle />
  <DropdownMenu
    trigger={
      <>
        <User className="w-4 h-4" />
        <span className="max-w-[120px] truncate">{user?.username}</span>
      </>
    }
    items={(() => {
      const menuItems: DropdownMenuItem[] = [
        {
          label: 'Settings',
          icon: <Settings className="w-4 h-4" />,
          onClick: () => navigate('/settings'),
        },
      ];
      if (user?.is_admin) {
        menuItems.push({
          label: 'Admin',
          icon: <Shield className="w-4 h-4" />,
          onClick: () => navigate('/admin'),
        });
      }
      menuItems.push(
        { label: 'Theme', icon: <SunMoon className="w-4 h-4" />, onClick: () => {} },
        {
          label: 'Logout',
          icon: <LogOut className="w-4 h-4" />,
          onClick: () => { logout(); navigate('/login', { replace: true }); },
          danger: true,
        },
      );
      return menuItems;
    })()}
  />
</div>

// NOTE: ThemeToggle stays as a standalone button above the dropdown.
// Remove the inline Logout button and "Logged in as" span entirely.
// Remove unused imports — keep ThemeToggle import, add lucide-react imports.
```

**Verification:**
- `npm run build` (TypeScript compiles cleanly)

---

### Task T2: Restructure Settings page to accordion sections (P4-G)

**Files:**
- Modify: `web-ui/src/pages/Settings.tsx`

**Depends on:** [] (independent of T0-T1 in code, but same phase)

**Breaking:** false

**Acceptance Criteria:**
- given: user navigates to /settings
- when: page loads
- then: sections display as collapsible accordion cards in order: Profile, Password, Storage Usage, Storage Backends, Watermark, Preprocessing, OAuth
- given: user clicks a collapsed section header
- when: section not currently expanded
- then: section expands, showing its content
- given: section is expanded
- when: user clicks its header again
- then: section collapses
- given: URL has hash "#settings?section=storage"
- when: page loads
- then: Storage section auto-expands and scrolls into view
- given: viewport < 768px
- when: viewing settings
- then: only one section can be open at a time

**Regression:**
- `npm run build` (all existing settings functionality must still work)
- Profile save, password change, OAuth link, and all sub-component interactions must function

**Implementation:**

```tsx
// test_code: Manual browser testing + `npm run build`

// impl_code: Modify web-ui/src/pages/Settings.tsx
// Add accordion state + section rendering

// ADD at top of file after existing imports:
import { Settings, Database, Lock, HardDrive, Droplets, Image, Shield,
         ChevronDown } from 'lucide-react';

type SettingsSection = 'profile' | 'password' | 'storage-usage'
  | 'storage-configs' | 'watermark' | 'preprocessing' | 'oauth';

// ADD inside component:
const [expanded, setExpanded] = useState<SettingsSection | null>(() => {
  const hash = window.location.hash.replace('#settings?section=', '');
  const validSections: SettingsSection[] = [
    'profile', 'password', 'storage-usage', 'storage-configs',
    'watermark', 'preprocessing', 'oauth',
  ];
  return validSections.includes(hash as SettingsSection)
    ? (hash as SettingsSection) : 'profile';
});

const toggleSection = (section: SettingsSection) => {
  setExpanded((prev) => (prev === section ? null : section));
  window.history.replaceState(null, '', `#settings?section=${section}`);
};

// Each existing card wrapper becomes an accordion panel:
// <div className="rounded-lg border border-[var(--color-border)]
//                 bg-[var(--glass-bg)] backdrop-blur-sm">
//   <button type="button" onClick={() => toggleSection(sectionId)}
//     className="flex w-full items-center justify-between p-4 text-sm font-medium
//                text-[var(--color-text-primary)]">
//     <span className="flex items-center gap-2">{icon} {title}</span>
//     <ChevronDown className={`w-4 h-4 transition-transform
//       ${expanded === sectionId ? 'rotate-180' : ''}`} />
//   </button>
//   {expanded === sectionId && <div className="px-4 pb-4">{content}</div>}
// </div>

// Section icon mapping:
// profile → <User />, password → <Lock />, storage-usage → <HardDrive />
// storage-configs → <Database />, watermark → <Droplets />
// preprocessing → <Image />, oauth → <Shield />
//
// Replace the current single-column card layout with this accordion structure.
// Each original card becomes a section. Keep all original form logic intact.
```

**Verification:**
- `npm run build` (TypeScript compiles cleanly)

---

### Task T3: Create systemd service files (P4-H)

**Files:**
- Create: `scripts/pichost-api.service`
- Create: `scripts/pichost-worker.service`

**Depends on:** [] (P4-H Phase 1 — independent)

**Breaking:** false

**Acceptance Criteria:**
- given: a systemd-based Linux host
- when: service files are placed in `/etc/systemd/system/`
- then: `systemctl daemon-reload` succeeds and `systemctl status pichost-api` shows a valid unit (loaded, not running until start)
- given: pichost-api binary exists at `/opt/pichost/pichost-api`
- when: `systemctl start pichost-api`
- then: `systemctl is-active pichost-api` returns `active`
- given: pichost-worker binary exists
- when: `systemctl start pichost-worker`
- then: `systemctl is-active pichost-worker` returns `active`, and the worker starts consuming from the Redis queue

**Regression:**
- `cargo build --workspace` (Rust compiles; services reference the binaries)
- Existing docker-compose.yml deploys (services are NOT used by Docker — no Docker impact)

**Implementation:**

```ini
# test_code: Validate unit files on a test host:
# sudo cp scripts/pichost-*.service /etc/systemd/system/
# sudo systemctl daemon-reload
# systemctl status pichost-api pichost-worker  (should show loaded but not running)
# sudo systemctl start pichost-api pichost-worker
# systemctl is-active pichost-api  (expect: active)
# systemctl is-active pichost-worker  (expect: active)

# impl_code: scripts/pichost-api.service
[Unit]
Description=PicHost API Server
After=network.target postgresql.service redis.service
Wants=postgresql.service redis.service

[Service]
Type=simple
User=pichost
Group=pichost
WorkingDirectory=/opt/pichost
EnvironmentFile=/etc/pichost/.env
ExecStart=/opt/pichost/pichost-api
Restart=on-failure
RestartSec=5
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target

# impl_code: scripts/pichost-worker.service
[Unit]
Description=PicHost Background Worker
After=network.target postgresql.service redis.service pichost-api.service
Wants=postgresql.service redis.service

[Service]
Type=simple
User=pichost
Group=pichost
WorkingDirectory=/opt/pichost
EnvironmentFile=/etc/pichost/.env
ExecStart=/opt/pichost/pichost-worker
Restart=on-failure
RestartSec=10
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

**Verification:**
- `systemctl status pichost-api` (unit file loads correctly)
- `systemctl status pichost-worker` (unit file loads correctly)

---

### Task T4: Create install/uninstall scripts (P4-H)

**Files:**
- Create: `scripts/install.sh`
- Create: `scripts/uninstall.sh`

**Depends on:** [T3] (systemd service files — referenced by install.sh)

**Breaking:** false

**Acceptance Criteria:**
- given: a Linux system with systemd
- when: `sudo bash install.sh /opt/pichost /var/lib/pichost /etc/pichost`
- then: binaries copied, directories created, .env file initialized, T3's service files installed to /etc/systemd/system/, and daemon-reloaded
- given: services installed via install.sh
- when: `systemctl start pichost-api pichost-worker`
- then: `systemctl is-active pichost-api` returns `active` AND `curl -sf http://localhost:3000/api/health` returns HTTP 200
- given: services running
- when: `sudo bash uninstall.sh`
- then: services stopped/disabled, binaries removed, data/config directories preserved with warning
- given: non-systemd system
- when: running install.sh
- then: systemd steps skipped, manual management instructions printed

**Regression:**
- `bash -n scripts/install.sh` (no syntax errors)
- `bash -n scripts/uninstall.sh` (no syntax errors)

**Implementation:**

```bash
# test_code: Manual verification on a target Linux system:
# 1. sudo bash install.sh /tmp/pichost-test /tmp/pichost-test-data /tmp/pichost-test-config
# 2. systemctl is-active pichost-api && echo "PASS" || echo "Review"
# 3. curl -sf http://localhost:3000/api/health && echo "PASS" || echo "Review"
# 4. sudo bash uninstall.sh /tmp/pichost-test /tmp/pichost-test-data /tmp/pichost-test-config
# 5. [ ! -d /tmp/pichost-test ] && echo "PASS: binaries removed" || echo "FAIL"
# Regression: for f in scripts/*.sh; do bash -n "$f" && echo "PASS: $f" || echo "FAIL: $f"; done

# impl_code: scripts/install.sh (from design spec §9.3)
#!/bin/bash
set -euo pipefail

INSTALL_DIR="${1:-/opt/pichost}"
DATA_DIR="${2:-/var/lib/pichost}"
CONFIG_DIR="${3:-/etc/pichost}"
VERSION="${PICHOST_VERSION:-unknown}"

echo "PicHost v${VERSION} installing..."

# 1. Create directory structure
mkdir -p "$INSTALL_DIR" "$DATA_DIR" "$CONFIG_DIR"

# 2. Copy binaries
cp pichost-api pichost-worker "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR"/pichost-api "$INSTALL_DIR"/pichost-worker

# 3. Copy static assets
cp -r web-ui/dist "$INSTALL_DIR/"
cp -r migrations "$INSTALL_DIR/"
if [ -d nginx ]; then cp -r nginx "$INSTALL_DIR/"; fi

# 4. Initialize .env if not present
if [ ! -f "$CONFIG_DIR/.env" ]; then
    cp .env.example "$CONFIG_DIR/.env"
    echo ">> Please edit $CONFIG_DIR/.env to configure PicHost"
    echo ">> Required: PICHOST_AUTH_JWT_SECRET (min 32 chars)"
    echo ">> Required: PICHOST_DATABASE_URL, PICHOST_REDIS_URL"
fi

# 5. Prerequisite check
echo ">> Ensure PostgreSQL 18+ and Redis 8+ are installed and running"

# 6. Install systemd services (if available)
if command -v systemctl &>/dev/null; then
    sed -i "s|/opt/pichost|$INSTALL_DIR|g" scripts/pichost-api.service
    sed -i "s|/opt/pichost|$INSTALL_DIR|g" scripts/pichost-worker.service
    sed -i "s|/etc/pichost|$CONFIG_DIR|g" scripts/pichost-api.service
    sed -i "s|/etc/pichost|$CONFIG_DIR|g" scripts/pichost-worker.service
    cp scripts/pichost-api.service /etc/systemd/system/
    cp scripts/pichost-worker.service /etc/systemd/system/
    systemctl daemon-reload
    echo ">> systemd services installed"
    echo ">> Start:    systemctl start pichost-api pichost-worker"
    echo ">> Enable:   systemctl enable pichost-api pichost-worker"
else
    echo ">> (Non-systemd; manage manually)"
    echo ">> API:    $INSTALL_DIR/pichost-api"
    echo ">> Worker: $INSTALL_DIR/pichost-worker"
fi

echo "PicHost installation complete."

# impl_code: scripts/uninstall.sh (from design spec §9.4)
#!/bin/bash
set -euo pipefail

INSTALL_DIR="${1:-/opt/pichost}"
DATA_DIR="${2:-/var/lib/pichost}"
CONFIG_DIR="${3:-/etc/pichost}"

echo "PicHost uninstalling..."

# 1. Stop and disable systemd services
if command -v systemctl &>/dev/null; then
    systemctl stop pichost-api pichost-worker 2>/dev/null || true
    systemctl disable pichost-api pichost-worker 2>/dev/null || true
    rm -f /etc/systemd/system/pichost-api.service
    rm -f /etc/systemd/system/pichost-worker.service
    systemctl daemon-reload
fi

# 2. Remove binaries and static files
rm -rf "$INSTALL_DIR"

# 3. Preserve data and config
echo ">> Binaries removed"
echo ">> Data dir preserved:   $DATA_DIR"
echo ">> Config dir preserved: $CONFIG_DIR"
echo ">> To fully remove: rm -rf $DATA_DIR $CONFIG_DIR"
echo "PicHost uninstall complete."
```

**Verification:**
- `for f in scripts/install.sh scripts/uninstall.sh; do bash -n "$f" && echo "PASS: $f" || echo "FAIL: $f"; done`

---

### Task T5: Create GitHub Actions release workflow + update .env.example (P4-H)

**Files:**
- Create: `.github/workflows/release.yml`
- Modify: `.env.example`

**Depends on:** [] (independent)

**Breaking:** false

**Acceptance Criteria:**
- given: a `v*` tag is pushed (e.g. `v0.17.0`)
- when: CI triggers
- then: builds Rust binary for `x86_64-unknown-linux-gnu`, frontend via `npm run build`, packages into `.tar.gz`, creates GitHub Release with artifacts
- given: `.env.example` is read
- when: user copies it
- then: all env vars needed for a minimal deployment are documented (DATABASE_URL, REDIS_URL, JWT_SECRET, PUBLIC_URL, STORAGE vars, AUTH vars)

**Regression:**
- `npm run build` (frontend must still build)
- `cargo build --workspace` (Rust must still compile)

**Implementation:**

```yaml
# test_code: No unit test — verify by pushing a test tag to a fork
# Regression: cargo test --workspace && npm run build

# impl_code: .github/workflows/release.yml (from design spec §9.6)
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build:
    name: Build (${{ matrix.target }})
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-24.04
            arch: amd64

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: 'npm'
          cache-dependency-path: web-ui/package-lock.json

      - name: Build frontend
        run: |
          cd web-ui
          npm ci
          npm run build

      - name: Build backend
        run: |
          cargo build --release --target ${{ matrix.target }} -p pichost-api -p pichost-worker

      - name: Strip binaries
        run: |
          strip target/${{ matrix.target }}/release/pichost-api
          strip target/${{ matrix.target }}/release/pichost-worker

      - name: Test
        run: cargo test --workspace

      - name: Lint
        run: cargo clippy --workspace -- -D warnings

      - name: Package
        run: |
          VERSION=${GITHUB_REF#refs/tags/}
          PKG_NAME="pichost-${VERSION}-${{ matrix.arch }}"
          mkdir -p dist/$PKG_NAME
          cp target/${{ matrix.target }}/release/pichost-api dist/$PKG_NAME/
          cp target/${{ matrix.target }}/release/pichost-worker dist/$PKG_NAME/
          cp -r web-ui/dist dist/$PKG_NAME/
          cp -r migrations dist/$PKG_NAME/
          cp -r nginx dist/$PKG_NAME/
          cp .env.example dist/$PKG_NAME/
          cp scripts/install.sh dist/$PKG_NAME/
          cp scripts/uninstall.sh dist/$PKG_NAME/
          cp scripts/pichost-api.service dist/$PKG_NAME/
          cp scripts/pichost-worker.service dist/$PKG_NAME/
          cp README.md dist/$PKG_NAME/
          cd dist && tar czf "${PKG_NAME}.tar.gz" "$PKG_NAME"

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: pichost-${{ matrix.arch }}
          path: dist/*.tar.gz

  release:
    name: Create Release
    needs: build
    runs-on: ubuntu-24.04
    permissions:
      contents: write

    steps:
      - uses: actions/download-artifact@v4

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          name: "PicHost ${{ github.ref_name }}"
          body: |
            ## PicHost ${{ github.ref_name }}

            ### Install

            ```bash
            tar xzf pichost-${{ github.ref_name }}-amd64.tar.gz
            cd pichost-${{ github.ref_name }}-amd64
            sudo bash install.sh
            ```

            ### Architecture Support

            | Arch | Filename |
            |------|----------|
            | x86_64 (amd64) | `pichost-${{ github.ref_name }}-amd64.tar.gz` |

            ### Changes

            See [CHANGELOG](https://github.com/JeillZhang/pichost/blob/main/CHANGELOG.md)
          files: |
            pichost-amd64/dist/*.tar.gz
          draft: false
          prerelease: false
```

```bash
# impl_code: Append to .env.example — add undocumented vars from AGENTS.md config table
# --- additions to existing .env.example ---

# Database
PICHOST_DATABASE_URL=postgresql://user:password@localhost:5432/pichost

# Redis
PICHOST_REDIS_URL=redis://localhost:6379

# Storage (local)
PICHOST_STORAGE_LOCAL_BASE_PATH=./storage-local

# Storage (S3/RustFS, optional)
# PICHOST_STORAGE_RUSTFS_ENDPOINT=https://s3.example.com
# PICHOST_STORAGE_RUSTFS_BUCKET=pichost
# PICHOST_STORAGE_RUSTFS_REGION=us-east-1
# PICHOST_STORAGE_RUSTFS_ACCESS_KEY=your-access-key
# PICHOST_STORAGE_RUSTFS_SECRET_KEY=your-secret-key

# Git storage (optional)
# PICHOST_AUTH_TOKEN_ENCRYPTION_KEY=your-32-byte-base64-encoded-key
# PICHOST_STORAGE_MAX_USER_CONFIGS=5

# Docker helper (sqlx CLI, not consumed by app)
DATABASE_URL=postgresql://user:password@localhost:5432/pichost
```

**Verification:**
- `cargo test --workspace` (all existing tests pass)
- `cargo clippy --workspace -- -D warnings` (zero warnings)
- `npm run build` (frontend builds)

---

### Task T6: Create config service for reading/writing config.toml (P4-I)

**Files:**
- Create: `pichost-api/src/services/config.rs`
- Test: inline `#[cfg(test)] mod tests` at bottom of config.rs

**Depends on:** [] (P4-I Phase 1 — independent of P4-G and P4-H)

**Breaking:** false

**Acceptance Criteria:**
- given: a valid `SystemConfig` struct
- when: `write_config_toml()` is called
- then: writes valid TOML to `config.toml`, preserving existing structure
- given: `config.toml` already exists at runtime path
- when: `read_config_toml()` is called
- then: returns `SystemConfig` with current values
- given: `config.toml` does not exist
- when: `read_config_toml()` is called
- then: returns `SystemConfig` with default (None) values
- given: a valid database_url
- when: `test_database_connection()` is called
- then: returns Ok(()) on connect + PING, 5s timeout
- given: a valid redis_url
- when: `test_redis_connection()` is called
- then: returns Ok(()) on PONG

**Regression:**
- `cargo test -p pichost-api test_config_service_write_and_read`
- `cargo test -p pichost-api test_config_service_defaults`

**Implementation:**

```rust
// test_code: pichost-api/src/services/config.rs — unit tests at bottom
// Write FIRST, verify they FAIL, then implement:

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_and_read_config_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = SystemConfig {
            database_url: Some("postgresql://test:test@localhost/test".into()),
            redis_url: Some("redis://localhost:6379".into()),
            jwt_secret: None,
            token_encryption_key: None,
            public_url: Some("https://pichost.example.com".into()),
            default_backend: Some("local".into()),
            local_base_path: Some("./test-storage".into()),
        };
        write_config_toml(&path, &config).unwrap();
        let read = read_config_toml(&path).unwrap();
        assert_eq!(read.database_url, config.database_url);
        assert_eq!(read.public_url, config.public_url);
    }

    #[test]
    fn test_read_defaults_when_no_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let config = read_config_toml(&path).unwrap();
        assert_eq!(config.database_url, None);
        assert_eq!(config.public_url, None);
    }
}
```

```rust
// impl_code: pichost-api/src/services/config.rs

use std::path::Path;

/// System configuration values manageable via admin UI.
/// Only non-sensitive, restart-required fields.
/// Sensitive fields (jwt_secret, token_encryption_key) are read-only.
#[derive(Debug, Clone, Default)]
pub struct SystemConfig {
    pub database_url: Option<String>,
    pub redis_url: Option<String>,
    pub jwt_secret: Option<String>,
    pub token_encryption_key: Option<String>,
    pub public_url: Option<String>,
    pub default_backend: Option<String>,
    pub local_base_path: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Connection error: {0}")]
    Connection(String),
}

/// Read current config from config.toml, matching figment's nested key structure.
/// Keys: database.url, redis.url, server.public_url,
///       storage.default_backend, storage.local_base_path, auth.jwt_secret.
pub fn read_config_toml(path: &Path) -> Result<SystemConfig, ConfigError> {
    if !path.exists() {
        return Ok(SystemConfig::default());
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::Io(e.to_string()))?;
    let doc: toml_edit::DocumentMut = content
        .parse()
        .map_err(|e| ConfigError::Parse(e.to_string()))?;

    fn get_str(doc: &toml_edit::DocumentMut, section: &str, key: &str) -> Option<String> {
        doc.get(section)
            .and_then(|t| t.get(key))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    Ok(SystemConfig {
        database_url: get_str(&doc, "database", "url"),
        redis_url: get_str(&doc, "redis", "url"),
        jwt_secret: get_str(&doc, "auth", "jwt_secret"),
        token_encryption_key: None, // not stored in config.toml
        public_url: get_str(&doc, "server", "public_url"),
        default_backend: get_str(&doc, "storage", "default_backend"),
        local_base_path: get_str(&doc, "storage", "local_base_path"),
    })
}

/// Write SystemConfig to config.toml using figment-compatible nested keys.
/// Preserves all existing sections/keys (including untouched ones).
pub fn write_config_toml(path: &Path, config: &SystemConfig) -> Result<(), ConfigError> {
    let mut doc: toml_edit::DocumentMut = if path.exists() {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        content.parse().map_err(|e| ConfigError::Parse(e.to_string()))?
    } else {
        toml_edit::DocumentMut::new()
    };

    fn set_nested(
        doc: &mut toml_edit::DocumentMut,
        section: &str,
        key: &str,
        val: &Option<String>,
    ) {
        match val {
            Some(v) => {
                doc[section][key] = toml_edit::value(v.as_str());
            }
            None => {
                if let Some(table) = doc.get_mut(section) {
                    table.as_table_mut().map(|t| t.remove(key));
                }
            }
        }
    }

    // Writes keys matching figment's Toml::file("config.toml").nested() expectation:
    //   [database] url = "..."
    //   [redis]    url = "..."
    //   [server]   public_url = "..."
    //   [storage]  default_backend = "..."
    //   [storage]  local_base_path = "..."
    set_nested(&mut doc, "database", "url", &config.database_url);
    set_nested(&mut doc, "redis", "url", &config.redis_url);
    set_nested(&mut doc, "server", "public_url", &config.public_url);
    set_nested(&mut doc, "storage", "default_backend", &config.default_backend);
    set_nested(&mut doc, "storage", "local_base_path", &config.local_base_path);
    // Sensitive fields intentionally excluded from writes

    std::fs::write(path, doc.to_string())
        .map_err(|e| ConfigError::Io(e.to_string()))
}

pub fn backup_config(path: &Path) -> Result<String, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::Io("config.toml not found".into()));
    }
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let backup_name = format!("config.toml.{}.bak", ts);
    let backup_path = path.parent().unwrap_or(Path::new(".")).join(&backup_name);
    std::fs::copy(path, &backup_path)
        .map_err(|e| ConfigError::Io(e.to_string()))?;
    Ok(backup_name)
}

pub fn list_backups(dir: &Path) -> Result<Vec<String>, ConfigError> {
    let mut backups = Vec::new();
    let entries = std::fs::read_dir(dir)
        .map_err(|e| ConfigError::Io(e.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|e| ConfigError::Io(e.to_string()))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("config.toml.") && name.ends_with(".bak") {
            backups.push(name);
        }
    }
    backups.sort_by(|a, b| b.cmp(a));
    Ok(backups)
}

pub fn restore_config(path: &Path, backup_file: &str) -> Result<(), ConfigError> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let backup_path = dir.join(backup_file);
    if !backup_path.exists() {
        return Err(ConfigError::Io(format!("Backup not found: {}", backup_file)));
    }
    std::fs::copy(&backup_path, path)
        .map_err(|e| ConfigError::Io(e.to_string()))?;
    Ok(())
}

pub async fn test_database_connection(url: &str) -> Result<(), ConfigError> {
    use tokio::time::timeout;
    use std::time::Duration;
    let result = timeout(Duration::from_secs(5), sqlx::PgPool::connect(url)).await;
    match result {
        Ok(Ok(pool)) => {
            sqlx::query("SELECT 1").execute(&pool).await
                .map_err(|e| ConfigError::Connection(e.to_string()))?;
            pool.close().await;
            Ok(())
        }
        Ok(Err(e)) => Err(ConfigError::Connection(e.to_string())),
        Err(_) => Err(ConfigError::Connection("timed out (5s)".into())),
    }
}

pub fn test_redis_connection(url: &str) -> Result<(), ConfigError> {
    let client = redis::Client::open(url)
        .map_err(|e| ConfigError::Connection(e.to_string()))?;
    let mut conn = client.get_connection()
        .map_err(|e| ConfigError::Connection(e.to_string()))?;
    let result: String = redis::cmd("PING").query(&mut conn)
        .map_err(|e| ConfigError::Connection(e.to_string()))?;
    if result == "PONG" { Ok(()) }
    else { Err(ConfigError::Connection(format!("unexpected PING: {}", result))) }
}
```

**Dependencies to add to `pichost-api/Cargo.toml`:**
```toml
toml_edit = "0.22"
regex = "1"
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
tempfile = "3"  # dev-dependency for tests
```

**Add to `pichost-api/src/services/mod.rs`:**
```rust
pub mod config;
```

**Verification:**
- `cargo test -p pichost-api test_config_service_write_and_read -- --nocapture`
- `cargo test -p pichost-api test_config_service_defaults -- --nocapture`
- `cargo clippy -p pichost-api -- -D warnings`

---

### Task T7: Create admin config API endpoints (P4-I)

**Files:**
- Modify: `pichost-api/src/routes/admin.rs` (add 6 handlers + request/response types)
- Modify: `pichost-api/src/main.rs` (`admin_routes()` function, add 6 route registrations)

**Depends on:** [T6] (config service)

**Breaking:** false

**Acceptance Criteria:**
- given: admin user → `GET /api/v1/admin/config` → returns current config with sensitive fields masked
- given: admin user → `PUT /api/v1/admin/config` with `{ "database_url": "..." }` → writes config.toml with auto-backup, returns 200
- given: admin user → `POST /api/v1/admin/config/test` with `{ "database_url": "..." }` → tests connection, returns `{ "database": "ok" }` or error
- given: admin user → `POST /api/v1/admin/config/backup` → creates backup, returns filename
- given: admin user → `GET /api/v1/admin/config/backups` → returns list of backup filenames
- given: admin user → `POST /api/v1/admin/config/restore` with `{ "backup_file": "..." }` → restores, returns 200
- given: non-admin user → any config endpoint → 403 Forbidden

**Regression:**
- `cargo test -p pichost-api test_admin_can_list_users -- --exact`
- `cargo test -p pichost-api test_admin_stats -- --exact`
- All existing admin routes must still work

**Implementation:**

```rust
// test_code: In admin.rs, add #[ignore] integration tests
// (existing admin tests are already #[ignore] — follow same pattern)

#[cfg(test)]
mod config_tests {
    use super::*;
    #[tokio::test]
    #[ignore] // Needs full server setup (DB + Redis + admin auth)
    async fn test_non_admin_cannot_access_config() {
        // Setup regular user → GET /admin/config → expect 403
    }
    #[tokio::test]
    #[ignore]
    async fn test_admin_can_get_config() {
        // Setup admin user → GET /admin/config → expect 200 + masked secrets
    }
}

// impl_code: ADD to pichost-api/src/routes/admin.rs (after existing handlers)

use crate::services::config;

// --- Request/Response types ---

#[derive(Debug, Deserialize)]
pub struct UpdateConfigBody {
    pub database_url: Option<String>,
    pub redis_url: Option<String>,
    pub public_url: Option<String>,
    pub default_backend: Option<String>,
    pub local_base_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TestConfigBody {
    pub database_url: Option<String>,
    pub redis_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RestoreBackupBody {
    pub backup_file: String,
}

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub token_encryption_key: String,
    pub public_url: String,
    pub default_backend: String,
    pub local_base_path: String,
    pub config_path: String,
}

#[derive(Debug, Serialize)]
pub struct BackupInfo { pub filename: String }

#[derive(Debug, Serialize)]
pub struct TestResult {
    pub database: Option<String>,
    pub redis: Option<String>,
}

fn mask_url(url: &str) -> String {
    let re = regex::Regex::new(r"://([^:]*):([^@]*)@").unwrap();
    re.replace(url, "://$1:***@").to_string()
}

fn config_file_path() -> std::path::PathBuf {
    std::env::current_dir()
        .map(|p| p.join("config.toml"))
        .unwrap_or_else(|_| std::path::PathBuf::from("config.toml"))
}

fn internal_error(msg: String) -> AdminError {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": msg})))
}

// --- Handlers ---

pub async fn get_admin_config(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<ConfigResponse>, AdminError> {
    let path = config_file_path();
    let cfg = config::read_config_toml(&path)
        .map_err(|e| internal_error(e.to_string()))?;

    Ok(Json(ConfigResponse {
        database_url: cfg.database_url.as_deref().map(mask_url)
            .unwrap_or_else(|| "not set".into()),
        redis_url: cfg.redis_url.as_deref().map(mask_url)
            .unwrap_or_else(|| "not set".into()),
        jwt_secret: "********".into(),
        token_encryption_key: if cfg.token_encryption_key.is_some()
            { "********".into() } else { "not set".into() },
        public_url: cfg.public_url.unwrap_or_else(|| "not set".into()),
        default_backend: cfg.default_backend.unwrap_or_else(|| "local".into()),
        local_base_path: cfg.local_base_path.unwrap_or_else(|| "./storage-local".into()),
        config_path: path.display().to_string(),
    }))
}

pub async fn update_admin_config(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<UpdateConfigBody>,
) -> Result<Json<ConfigResponse>, AdminError> {
    let path = config_file_path();
    // Best-effort backup — first-save may have no existing config.toml
    let _ = config::backup_config(&path);

    let cfg = config::SystemConfig {
        database_url: body.database_url,
        redis_url: body.redis_url,
        jwt_secret: None,
        token_encryption_key: None,
        public_url: body.public_url,
        default_backend: body.default_backend,
        local_base_path: body.local_base_path,
    };
    config::write_config_toml(&path, &cfg)
        .map_err(|e| internal_error(format!("write failed: {}", e)))?;

    get_admin_config(State(_state)).await
}

pub async fn test_admin_config(
    Json(body): Json<TestConfigBody>,
) -> Result<Json<TestResult>, AdminError> {
    let mut result = TestResult { database: None, redis: None };
    if let Some(ref url) = body.database_url {
        result.database = Some(match config::test_database_connection(url).await {
            Ok(()) => "ok".into(),
            Err(e) => format!("fail: {}", e),
        });
    }
    if let Some(ref url) = body.redis_url {
        result.redis = Some(match config::test_redis_connection(url) {
            Ok(()) => "ok".into(),
            Err(e) => format!("fail: {}", e),
        });
    }
    Ok(Json(result))
}

pub async fn backup_admin_config() -> Result<Json<BackupInfo>, AdminError> {
    let path = config_file_path();
    let filename = config::backup_config(&path)
        .map_err(|e| internal_error(e.to_string()))?;
    Ok(Json(BackupInfo { filename }))
}

pub async fn list_config_backups() -> Result<Json<Vec<BackupInfo>>, AdminError> {
    let dir = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let backups = config::list_backups(&dir)
        .map_err(|e| internal_error(e.to_string()))?
        .into_iter()
        .map(|filename| BackupInfo { filename })
        .collect();
    Ok(Json(backups))
}

pub async fn restore_admin_config(
    Json(body): Json<RestoreBackupBody>,
) -> Result<Json<serde_json::Value>, AdminError> {
    let path = config_file_path();
    config::restore_config(&path, &body.backup_file)
        .map_err(|e| internal_error(e.to_string()))?;
    Ok(Json(serde_json::json!({"status": "restored", "from": body.backup_file})))
}
```

```rust
// impl_code: Modify pichost-api/src/main.rs — inside admin_routes() function
// ADD these routes BEFORE the .route_layer chain (around line 160):

.route("/config", get(routes::admin::get_admin_config)
    .put(routes::admin::update_admin_config))
.route("/config/test", post(routes::admin::test_admin_config))
.route("/config/backup", post(routes::admin::backup_admin_config))
.route("/config/backups", get(routes::admin::list_config_backups))
.route("/config/restore", post(routes::admin::restore_admin_config))
```

**Verification:**
- `cargo check -p pichost-api` (compiles without errors)
- `cargo test -p pichost-api test_admin_can_list_users -- --exact` (existing admin tests pass)
- `cargo clippy --workspace -- -D warnings`

---

### Task T8: Create SystemConfig frontend component (P4-I)

**Files:**
- Create: `web-ui/src/components/SystemConfig.tsx`

**Depends on:** [T7] (admin config API endpoints)

**Breaking:** false

**Acceptance Criteria:**
- given: admin user viewing System Config tab
- when: page loads
- then: displays current config values with sensitive fields masked
- given: admin edits database_url and clicks "Test Connection"
- when: test returns
- then: shows ✓ "Connection OK" or ✗ error next to the field
- given: admin clicks "Save and Restart Required"
- when: save completes
- then: shows success toast, config refreshes
- given: admin clicks "Backup Current Config"
- when: backup completes
- then: shows success toast, backup list refreshes
- given: admin selects a backup and clicks restore
- when: restore completes
- then: config fields refresh to restored values

**Regression:**
- `npm run build` (all existing components must compile)

**Implementation:**

```tsx
// test_code: Manual browser testing + `npm run build`

// impl_code: web-ui/src/components/SystemConfig.tsx

import { useState, useEffect } from 'react';
import ky from 'ky';
import { toast } from 'sonner';
import {
  Database, HardDrive, Globe, Key, Server, Shield,
  RotateCcw, Save, TestTube,
} from 'lucide-react';

interface ConfigData {
  database_url: string;
  redis_url: string;
  jwt_secret: string;
  token_encryption_key: string;
  public_url: string;
  default_backend: string;
  local_base_path: string;
  config_path: string;
}

interface TestResult {
  database: string | null;
  redis: string | null;
}

interface BackupInfo { filename: string }

export default function SystemConfig() {
  const [config, setConfig] = useState<ConfigData | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [dbUrl, setDbUrl] = useState('');
  const [redisUrl, setRedisUrl] = useState('');
  const [publicUrl, setPublicUrl] = useState('');
  const [defaultBackend, setDefaultBackend] = useState('');
  const [localPath, setLocalPath] = useState('');
  const [dbTest, setDbTest] = useState<string | null>(null);
  const [redisTest, setRedisTest] = useState<string | null>(null);
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [backingUp, setBackingUp] = useState(false);

  useEffect(() => { loadConfig(); loadBackups(); }, []);

  async function loadConfig() {
    try {
      const data = await ky.get('/api/v1/admin/config').json<ConfigData>();
      setConfig(data);
      setDbUrl(data.database_url);
      setRedisUrl(data.redis_url);
      setPublicUrl(data.public_url);
      setDefaultBackend(data.default_backend);
      setLocalPath(data.local_base_path);
    } catch { toast.error('Failed to load config'); }
    finally { setLoading(false); }
  }

  async function loadBackups() {
    try {
      const data = await ky.get('/api/v1/admin/config/backups').json<BackupInfo[]>();
      setBackups(data);
    } catch { /* non-critical */ }
  }

  async function testConnection(type: 'database' | 'redis') {
    const body: { database_url?: string; redis_url?: string } = {};
    if (type === 'database') body.database_url = dbUrl;
    if (type === 'redis') body.redis_url = redisUrl;
    try {
      const result = await ky.post('/api/v1/admin/config/test', { json: body }).json<TestResult>();
      setDbTest(result.database ?? null);
      setRedisTest(result.redis ?? null);
    } catch { toast.error('Connection test failed'); }
  }

  async function saveConfig() {
    setSaving(true);
    try {
      await ky.put('/api/v1/admin/config', {
        json: {
          database_url: dbUrl !== config?.database_url ? dbUrl : undefined,
          redis_url: redisUrl !== config?.redis_url ? redisUrl : undefined,
          public_url: publicUrl !== config?.public_url ? publicUrl : undefined,
          default_backend: defaultBackend !== config?.default_backend ? defaultBackend : undefined,
          local_base_path: localPath !== config?.local_base_path ? localPath : undefined,
        },
      });
      toast.success('Config saved. Restart service to apply.');
      loadConfig();
    } catch { toast.error('Failed to save config'); }
    finally { setSaving(false); }
  }

  async function handleBackup() {
    setBackingUp(true);
    try {
      const result = await ky.post('/api/v1/admin/config/backup').json<BackupInfo>();
      toast.success(`Backup: ${result.filename}`);
      loadBackups();
    } catch { toast.error('Backup failed'); }
    finally { setBackingUp(false); }
  }

  async function handleRestore(filename: string) {
    try {
      await ky.post('/api/v1/admin/config/restore', { json: { backup_file: filename } });
      toast.success(`Restored from ${filename}`);
      loadConfig();
    } catch { toast.error('Restore failed'); }
  }

  if (loading) return <div className="p-4 text-sm text-[var(--color-text-muted)]">Loading...</div>;

  const card = "rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm";
  const header = "flex items-center gap-2 text-sm font-medium text-[var(--color-text-primary)] mb-3";
  const input = "w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm text-[var(--color-text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--color-accent)]";
  const label = "block text-xs text-[var(--color-text-muted)] mb-1";
  const btn = "flex items-center gap-1 rounded-lg border border-[var(--color-border)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)]";

  return (
    <div className="space-y-4">
      {/* Database */}
      <div className={card}>
        <div className={header}><Database className="w-4 h-4" />Database</div>
        <label className={label}>PostgreSQL URL</label>
        <div className="flex gap-2">
          <input value={dbUrl} onChange={e => setDbUrl(e.target.value)} className={`${input} flex-1`} />
          <button onClick={() => testConnection('database')} className={btn}><TestTube className="w-3 h-3" />Test</button>
        </div>
        {dbTest && <p className={`mt-1 text-xs ${dbTest === 'ok' ? 'text-green-400' : 'text-red-400'}`}>{dbTest === 'ok' ? '✓ Connection OK' : `✗ ${dbTest}`}</p>}
      </div>

      {/* Redis */}
      <div className={card}>
        <div className={header}><Server className="w-4 h-4" />Redis</div>
        <label className={label}>Redis URL</label>
        <div className="flex gap-2">
          <input value={redisUrl} onChange={e => setRedisUrl(e.target.value)} className={`${input} flex-1`} />
          <button onClick={() => testConnection('redis')} className={btn}><TestTube className="w-3 h-3" />Test</button>
        </div>
        {redisTest && <p className={`mt-1 text-xs ${redisTest === 'ok' ? 'text-green-400' : 'text-red-400'}`}>{redisTest === 'ok' ? '✓ Connection OK' : `✗ ${redisTest}`}</p>}
      </div>

      {/* Server */}
      <div className={card}>
        <div className={header}><Globe className="w-4 h-4" />Server</div>
        <label className={label}>Public URL</label>
        <input value={publicUrl} onChange={e => setPublicUrl(e.target.value)} className={input} />
        <label className={`${label} mt-3`}>Default Storage Backend</label>
        <select value={defaultBackend} onChange={e => setDefaultBackend(e.target.value)} className={input}>
          <option value="local">local</option>
          <option value="rustfs">rustfs (S3)</option>
        </select>
        <label className={`${label} mt-3`}>Local Storage Path</label>
        <input value={localPath} onChange={e => setLocalPath(e.target.value)} className={input} />
      </div>

      {/* Security (read-only) */}
      <div className={card}>
        <div className={header}><Shield className="w-4 h-4" />Security (Read-Only)</div>
        <label className={label}>JWT Secret</label>
        <input value={config?.jwt_secret ?? ''} readOnly className={`${input} opacity-50 cursor-not-allowed`} />
        <label className={`${label} mt-3`}>Token Encryption Key</label>
        <input value={config?.token_encryption_key ?? ''} readOnly className={`${input} opacity-50 cursor-not-allowed`} />
      </div>

      {/* Warning */}
      <div className="rounded-lg border border-yellow-500/30 bg-yellow-500/5 p-3">
        <p className="text-xs text-yellow-400">⚠ Changes require a service restart to take effect.</p>
      </div>

      {/* Actions */}
      <div className="flex items-center gap-2">
        <button onClick={saveConfig} disabled={saving}
                className="flex items-center gap-1.5 rounded-lg bg-[var(--color-accent)] px-4 py-2 text-sm font-medium text-white hover:opacity-90 disabled:opacity-50">
          <Save className="w-4 h-4" />{saving ? 'Saving...' : 'Save and Restart Required'}
        </button>
      </div>

      {/* Backup & Restore */}
      <div className={card}>
        <div className={header}><RotateCcw className="w-4 h-4" />Backup & Restore</div>
        <div className="flex items-center gap-2 mb-3">
          <button onClick={handleBackup} disabled={backingUp} className={btn}>
            <Save className="w-3 h-3" />{backingUp ? 'Backing up...' : 'Backup Current Config'}
          </button>
        </div>
        {backups.length > 0 && (
          <div>
            <p className="text-xs text-[var(--color-text-muted)] mb-2">Restore from backup:</p>
            <div className="space-y-1">
              {backups.map(b => (
                <button key={b.filename} onClick={() => handleRestore(b.filename)}
                        className="block w-full text-left rounded-lg px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)] border border-transparent hover:border-[var(--color-border)]">
                  {b.filename}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
```

**Verification:**
- `npm run build` (TypeScript compiles cleanly)

---

### Task T9: Add System Config tab to Admin page (P4-I)

**Files:**
- Modify: `web-ui/src/pages/Admin.tsx` (extend Tab type, add tab button, add import, add conditional render)

**Depends on:** [T8] (SystemConfig component)

**Breaking:** false

**Acceptance Criteria:**
- given: admin user viewing Admin page
- when: page loads
- then: tab bar shows Overview, Users, Invites, Config
- given: admin clicks "Config" tab
- when: tab activates
- then: SystemConfig component renders
- given: admin clicks back to "Overview"
- when: navigating away
- then: AdminStats renders correctly

**Regression:**
- `npm run build` (Admin page must compile)
- Overview, Users, Invites tabs must still work

**Implementation:**

```tsx
// test_code: Manual browser testing + `npm run build`

// impl_code: Modify web-ui/src/pages/Admin.tsx

// STEP 1: Extend Tab type (~line 6):
// REPLACE: type Tab = 'overview' | 'users' | 'invites';
type Tab = 'overview' | 'users' | 'invites' | 'system-config';

// STEP 2: Add import:
import SystemConfig from '../components/SystemConfig';

// STEP 3: Add Config tab button in the segmented pill control
// INSERT after the Invites button's closing </button>:
<button type="button" onClick={() => setActiveTab('system-config')}
  className="flex-1 rounded-lg px-4 py-2 text-sm font-medium transition-colors"
  style={{
    backgroundColor: activeTab === 'system-config'
      ? 'var(--color-accent-subtle)' : 'transparent',
    color: activeTab === 'system-config'
      ? 'var(--color-accent)' : 'var(--color-text-muted)',
  }}>
  Config
</button>

// STEP 4: Add conditional render before closing fragment:
{activeTab === 'system-config' && <SystemConfig />}
```

**Verification:**
- `npm run build` (TypeScript compiles cleanly)

---

### Completion Checklist

After all tasks (T0–T9, 10 total) are implemented, run:

```bash
# Backend
cargo clippy --workspace -- -D warnings
cargo test --workspace

# Frontend
cd web-ui && npm run build

# Shell scripts
for f in scripts/*.sh; do bash -n "$f" && echo "PASS: $f"; done
```

**Version bumps:**
- P4-G → `0.16.3` → `0.16.4`
- P4-H → `0.16.4` → `0.17.0`
- P4-I → `0.17.0` → `0.17.1`
