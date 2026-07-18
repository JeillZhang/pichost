# Plan B: Frontend Visual Polish + Admin Panel

> **Date**: 2026-07-12
> **Project**: `pichost`
> **Branch**: `feat/planb-visual-admin`
> **Pre-plan Analysis**: Metis consulted — no shadcn/ui, no CVA, separate admin middleware

---

## Phase A: Backend — Admin Middleware & API Endpoints

Backend work is independent of frontend and can start immediately.

### Task A1: `require_admin` Middleware
- **File**: `pichost-api/src/middleware/auth.rs`
- **What**: New `require_admin` middleware function (alongside existing `require_auth`)
- **Logic**:
  1. Extract `AuthUser` from `req.extensions()` (already injected by `require_auth`)
  2. If `!user.is_admin` → return `403 Forbidden` with `{"error": "admin access required"}`
  3. If admin → pass through to `next.run(req).await`
- **Pattern**: Clone the `require_auth` structure, but check extensions instead of JWT
- **Expected**: ~25 LOC, single pure function

### Task A2: Admin API — `GET /api/v1/admin/users` (list users)
- **File**: `pichost-api/src/routes/admin.rs` (new)
- **What**: Paginated user list endpoint
- **Query params**: `?offset=0&limit=50`
- **SQL**: `SELECT id, username, email, is_admin, storage_backend, created_at FROM users ORDER BY created_at DESC OFFSET $1 LIMIT $2`
- **Response**: `{ users: UserInfo[], total: i64 }`
- **Protected**: JWT + admin (via `require_admin` middleware)

### Task A3: Admin API — `PATCH /api/v1/admin/users/:id` (update user)
- **File**: `pichost-api/src/routes/admin.rs`
- **What**: Modify user fields — `username`, `email`, `password`, `is_admin`, `storage_backend`
- **Body**: `{ username?: string, email?: string, password?: string, is_admin?: bool, storage_backend?: string }`
- **SQL**: Dynamic `UPDATE users SET ... WHERE id = $1` (only set provided fields)
- **Password**: If `password` provided → re-hash with Argon2id before UPDATE
- **Response**: Updated `UserInfo`
- **Validation**: Cannot demote self (admin who makes the request cannot toggle their own `is_admin` to false)

### Task A3b: Admin API — `DELETE /api/v1/admin/users/:id` (delete user)
- **File**: `pichost-api/src/routes/admin.rs`
- **What**: Delete user and all their images (cascade)
- **Logic**: 
  1. Verify user exists
  2. Delete user's physical image files from storage backends
  3. `DELETE FROM images WHERE user_id = $1`
  4. `DELETE FROM users WHERE id = $1`
- **Validation**: Cannot delete self
- **Response**: `204 No Content`

### Task A4: Admin API — `GET /api/v1/admin/stats` (system statistics)
- **File**: `pichost-api/src/routes/admin.rs`
- **What**: System-wide dashboard stats
- **Response**:
  ```json
  {
    "total_users": 45,
    "total_images": 1230,
    "total_size": 8800000000,
    "active_users_24h": 12,
    "storage_backends": {
      "local": { "total_images": 800, "total_size": 5000000000 },
      "rustfs": { "total_images": 430, "total_size": 3800000000 }
    }
  }
  ```
- **SQL**: Aggregate queries on `images` + `users` tables
- **Cache**: Wrap in Redis `cached_meta` pattern (TTL 300s — system stats can be slightly stale)

### Task A5: Route Registration in `main.rs`
- Add `admin_protected` middleware layer (combines `require_auth` + `require_admin`)
- Register routes: `.nest("/api/v1/admin", admin_routes)`
- Middleware stacking: `rate_limit_general` → `protected` → `require_admin`

### Task A6: `routes/mod.rs` + `middleware/mod.rs` exports
- Add `pub mod admin;` to `routes/mod.rs`
- Re-export `require_admin` from middleware module

### Task A7: Integration tests for admin endpoints
- **File**: `pichost-api/tests/admin_test.rs` (new)
- **Tests**:
  - Non-admin token → `GET /api/v1/admin/users` → 403
  - Admin token → `GET /api/v1/admin/users` → 200 with user list
  - Admin token → `PATCH /api/v1/admin/users/:id { "is_admin": true }` → 200
  - Admin demoting self → 400 or 403
  - Admin deleting self → 400 or 403
  - Admin token → `DELETE /api/v1/admin/users/:id` → 204, subsequent GET → 404
  - Admin token → `GET /api/v1/admin/stats` → 200 with stats object

---

## Phase B: Frontend — Theme Infrastructure

Theme system is a prerequisite for both glassmorphism (Phase C) and admin panel (Phase D).

### Task B1: CSS Variables + Theme Tokens
- **File**: `web-ui/src/theme.css` (new)
- **Content**: CSS custom properties for light and dark themes
  ```css
  :root {
    --color-bg: #030712;
    --color-surface: rgba(17, 24, 39, 0.5);
    --color-surface-elevated: rgba(17, 24, 39, 0.8);
    --color-border: rgba(75, 85, 99, 0.3);
    --color-text-primary: #f9fafb;
    --color-text-secondary: #9ca3af;
    --color-text-muted: #6b7280;
    --color-accent: #3b82f6;
    --color-accent-hover: #2563eb;
    --color-danger: #ef4444;
    --color-danger-hover: #dc2626;
    --glass-bg: rgba(255, 255, 255, 0.03);
    --glass-border: rgba(255, 255, 255, 0.06);
    --glass-blur: 12px;
    --radius-sm: 0.375rem;
    --radius-md: 0.5rem;
    --radius-lg: 0.75rem;
    --radius-xl: 1rem;
  }

  .light {
    --color-bg: #f9fafb;
    --color-surface: rgba(255, 255, 255, 0.7);
    --color-surface-elevated: rgba(255, 255, 255, 0.95);
    --color-border: rgba(209, 213, 219, 0.6);
    --color-text-primary: #111827;
    --color-text-secondary: #4b5563;
    --color-text-muted: #9ca3af;
    --color-accent: #2563eb;
    --color-accent-hover: #1d4ed8;
    --glass-bg: rgba(255, 255, 255, 0.5);
    --glass-border: rgba(0, 0, 0, 0.06);
  }
  ```
- **Integration**: `@import './theme.css'` in `index.css`

### Task B2: Tailwind v4 Dark Mode Strategy
- **File**: `web-ui/src/index.css`
- **Add**: `@variant dark (&:where(.dark, .dark *));` — enables `dark:` prefix classes
- **Add**: `@theme` block mapping CSS variables to Tailwind tokens

### Task B3: Theme Store (`stores/ui.ts`)
- **File**: `web-ui/src/stores/ui.ts` (new)
- **State**: `{ theme: 'light' | 'dark' | 'system' }`
- **Persistence**: `localStorage` key `"pichost-theme"`
- **Initialization**: Read from localStorage, fallback to `'system'`
- **Resolve logic**:
  - `'light'` → `document.documentElement.classList.remove('dark')`
  - `'dark'` → `document.documentElement.classList.add('dark')`
  - `'system'` → follow OS preference via `matchMedia`

### Task B4: Flash Prevention Script
- **File**: `web-ui/index.html` (in `<head>`, before any CSS/JS loads)
- **Script**: Inline `<script>` that reads `localStorage` → applies `dark` class to `<html>` BEFORE React renders

### Task B5: `ThemeToggle` Component
- **File**: `web-ui/src/components/ThemeToggle.tsx` (new)
- **UI**: Icon button with 3 icons (Sun, Moon, Monitor from lucide-react)
- **Behavior**: Click cycles: `light → dark → system → light`

### Task B6: Update `index.css` body styles
- Replace hardcoded `bg-gray-950 text-gray-100` with CSS variable references
- Apply `bg-[var(--color-bg)] text-[var(--color-text-primary)]`

---

## Phase C: Frontend — Glassmorphism Visual Polish

Apply glassmorphism to all existing pages and components.

### Task C1: `Layout` Component (Shared Shell)
- **File**: `web-ui/src/components/Layout.tsx` (new)
- **Wraps**: All protected pages
- **Children**: `<NavBar />` + `<main>{children}</main>`
- **Purpose**: Eliminates repeated NavBar import + page structure in every page

### Task C2: App.tsx Restructuring
- Wrap all protected routes in `<Layout>`
- Add `/admin` route (placeholder for Phase D)
- Keep Login page outside Layout (no navbar)

### Task C3: Glassmorphism — Login Page
- **File**: `web-ui/src/pages/Login.tsx`
- **Changes**: Form card → glass card, inputs → glass inputs, branding → gradient text

### Task C4: Glassmorphism — Dashboard Page
- **File**: `web-ui/src/pages/Dashboard.tsx`
- **Changes**: DropZone glass border, recent items glass cards, LinkCards glass

### Task C5: Glassmorphism — Gallery Page
- **File**: `web-ui/src/pages/Gallery.tsx`
- **Changes**: Image cards glass border, hover scale effect

### Task C6: Glassmorphism — ImageDetail Page
- **File**: `web-ui/src/pages/ImageDetail.tsx`
- **Changes**: Preview frame glass, action buttons glass variant

### Task C7: Glassmorphism — NavBar
- **File**: `web-ui/src/components/NavBar.tsx`
- **Changes**: Convert to CSS variables, add `<ThemeToggle />`, add admin nav link

### Task C8: Glassmorphism — DropZone + LinkCard
- **DropZone.tsx**: Glass border treatment
- **LinkCard.tsx**: Glass container

### Task C9: Extract `Button` Component
- **File**: `web-ui/src/components/ui/Button.tsx` (new)
- **Variants**: `primary`, `danger`, `ghost`, `ghost-sm`, `icon`
- **Purpose**: Consolidate 5 duplicated button patterns

### Task C10: Extract `Input` Component
- **File**: `web-ui/src/components/ui/Input.tsx` (new)
- **Pattern**: Single input pattern from Login.tsx, theme-aware

---

## Phase D: Frontend — Admin Panel

Depends on Phase B (theme system) and Phase A (backend endpoints).

### Task D1: `AdminRoute` Guard Component
- **File**: `web-ui/src/components/AdminRoute.tsx` (new)
- **Logic**: Check `user.is_admin` → if false, redirect to `/dashboard`

### Task D2: Admin Stats Dashboard
- **File**: `web-ui/src/pages/AdminStats.tsx` (new)
- **API**: `GET /api/v1/admin/stats`
- **UI**: 4 stat cards (users, images, storage, active today) + storage backend breakdown

### Task D3: Admin Users Table
- **File**: `web-ui/src/pages/AdminUsers.tsx` (new)
- **API**: `GET /api/v1/admin/users?offset=0&limit=50`
- **UI**: Glass table with columns: username, email, admin badge, created date, actions

### Task D4: Edit User Dialog
- **File**: `web-ui/src/components/EditUserDialog.tsx` (new)
- **API**: `PATCH /api/v1/admin/users/:id`
- **UI**: Glass modal with is_admin toggle, email input, storage_backend select

### Task D5: Admin Shell Page
- **File**: `web-ui/src/pages/Admin.tsx` (new)
- **Layout**: Tab navigation (Overview | Users) + content area

### Task D6: Route Registration
- Add `/admin` route to `App.tsx` wrapped in `<AdminRoute>` + `<Layout>`

---

## Phase E: Verification & Cleanup

### Task E1: Visual QA
- Screenshot all 5 pages in light + dark mode
- Verify no hardcoded dark colors bleeding through in light mode

### Task E2: Backend Test Run
- `cargo test --workspace` — all tests pass
- `cargo clippy --workspace -- -D warnings` — zero warnings

### Task E3: Frontend Build
- `cd web-ui && npm run build` — type check + Vite bundle pass

### Task E4: Spec Update
- Update design spec §15: mark "管理员面板" ✅ and "视觉打磨" ✅

---

## Dependency Graph

```
Phase A (backend) ──┐
                     ├──→ Phase D (admin frontend) ──→ Phase E (verify)
Phase B (theme) ────┤
                     └──→ Phase C (glassmorphism) ────→ Phase E (verify)
```

---

## Summary

| Phase | Tasks | Description |
|-------|-------|-------------|
| A | 8 | Backend: admin middleware + 4 API endpoints (list/update/delete/stats) + tests |
| B | 6 | Theme: CSS vars, dark mode, store, flash prevention, toggle |
| C | 10 | Glassmorphism: Layout, all pages, components, Button/Input extraction |
| D | 6 | Admin panel: guard, stats, users table, edit dialog, routing |
| E | 4 | Verification: visual QA, tests, build, spec update |
| **Total** | **34** | |

---

## Ambiguities (RESOLVED)

1. **User modification scope**: ✅ Full — `username`, `email`, `password`, `is_admin`, `storage_backend`
2. **User deletion**: ✅ Supported — admin can delete users (cascade deletes images + storage files)
3. **Admin layout**: ✅ Single `/admin` with tabs (Overview | Users)
4. **Theme default**: ✅ System (follows OS preference)
