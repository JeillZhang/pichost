# PicHost UI Visual Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the warm-stone/teal palette and flat glass system with a coherent slate-indigo professional palette and Apple-style tinted layered glassmorphism.

**Architecture:** Pure CSS/TSX token change — all visual identity flows from `theme.css` tokens consumed by `index.css` utility classes and component inline styles. No structural, layout, or logic changes.

**Tech Stack:** Tailwind CSS 4 (CSS-first, no JS config), React 19, TypeScript 7, Vite 8.

**Spec:** `docs/superpowers/specs/2026-08-02-pichost-ui-redesign-design.md`

## Global Constraints

- Zero functional/logic changes to any component
- Glass CSS class names (`.glass`, `.glass-elevated`, `.glass-nav`, `.glass-modal`) preserved — external interface unchanged, internal implementation changed
- Tailwind CSS 4 `@import "tailwindcss"` config unchanged
- Theme toggling mechanism (Zustand `useUiStore` + `.dark` class on `<html>`) unchanged
- `Inter` + `Outfit` fonts unchanged
- Radius, transition, focus-ring tokens unchanged
- No `cargo` commands (zero Rust code touched)
- Verify with `npm run build` after all changes

---

### Task 1: Rewrite theme.css — Light Theme Tokens

**Files:**
- Modify: `web-ui/src/theme.css`

**Interfaces:**
- Produces: `--color-bg`, `--color-text-primary`, `--color-accent`, `--glass-tint-base`, `--glass-layer-*-opacity/blur/saturate`, `--shadow-*` (consumed by Task 2 index.css and all TSX components)

- [ ] **Step 1: Replace light theme (:root) block with slate-indigo palette + glass layer tokens**

Replace the entire `:root { ... }` block (lines 13-93) with:

```css
/* ── Light Theme (default) ──────────────────── */
:root {
  /* ── Core Backgrounds ── */
  --color-bg: #f8fafc;
  --color-bg-subtle: #f1f5f9;
  --color-surface: rgba(15, 23, 42, 0.03);
  --color-surface-hover: rgba(15, 23, 42, 0.06);
  --color-surface-elevated: rgba(255, 255, 255, 0.94);
  --color-surface-glass: rgba(255, 255, 255, 0.42);

  /* ── Glassmorphism — Base (Light) ── */
  --glass-highlight: rgba(255, 255, 255, 0.55);
  --glass-tint-base: oklch(0.97 0.005 260);
  --glass-border-base: rgba(15, 23, 42, 0.05);
  --glass-border-strong: rgba(15, 23, 42, 0.08);

  /* Layer 1: Card */
  --glass-layer-card-opacity: 0.65;
  --glass-layer-card-blur: 16px;
  --glass-layer-card-saturate: 140%;

  /* Layer 2: Elevated (dropdown, detail card, sidebar) */
  --glass-layer-elevated-opacity: 0.78;
  --glass-layer-elevated-blur: 24px;
  --glass-layer-elevated-saturate: 150%;

  /* Layer 3: NavBar */
  --glass-layer-nav-opacity: 0.88;
  --glass-layer-nav-blur: 32px;
  --glass-layer-nav-saturate: 160%;

  /* Layer 4: Modal */
  --glass-layer-modal-opacity: 0.90;
  --glass-layer-modal-blur: 40px;
  --glass-layer-modal-saturate: 150%;

  /* Layer shadows (built with slate-900 base) */
  --glass-shadow:
    0 1px 3px rgba(15, 23, 42, 0.06),
    0 8px 24px rgba(15, 23, 42, 0.09);
  --glass-shadow-sm:
    0 1px 2px rgba(15, 23, 42, 0.04),
    0 2px 8px rgba(15, 23, 42, 0.06);

  /* ── Borders ── */
  --color-border: rgba(15, 23, 42, 0.06);
  --color-border-hover: rgba(15, 23, 42, 0.12);
  --color-border-strong: rgba(15, 23, 42, 0.08);

  /* ── Text — Slate scale ── */
  --color-text-primary: #0f172a;
  --color-text-secondary: #475569;
  --color-text-muted: #94a3b8;
  --color-text-on-accent: #ffffff;

  /* ── Accent — Indigo ── */
  --color-accent: #4f46e5;
  --color-accent-hover: #4338ca;
  --color-accent-active: #3730a3;
  --color-accent-subtle: rgba(79, 70, 229, 0.07);
  --color-accent-glow: rgba(79, 70, 229, 0.12);
  --color-accent-strong: rgba(79, 70, 229, 0.20);

  --color-accent-gradient: linear-gradient(135deg, #4f46e5 0%, #6366f1 50%, #4f46e5 100%);
  --color-accent-gradient-hover: linear-gradient(135deg, #4338ca 0%, #818cf8 50%, #4338ca 100%);

  /* ── Semantic Colors ── */
  --color-danger: #ef4444;
  --color-danger-hover: #dc2626;
  --color-danger-active: #b91c1c;
  --color-danger-subtle: rgba(239, 68, 68, 0.07);
  --color-danger-border: rgba(239, 68, 68, 0.15);

  --color-success: #16a34a;
  --color-success-hover: #15803d;
  --color-success-active: #166534;
  --color-success-subtle: rgba(22, 163, 74, 0.07);

  --color-warning: #f59e0b;
  --color-warning-hover: #d97706;
  --color-warning-subtle: rgba(245, 158, 11, 0.07);

  /* ── Radii ── */
  --radius-sm: 0.5rem;
  --radius-md: 0.75rem;
  --radius-lg: 1rem;
  --radius-xl: 1.25rem;
  --radius-2xl: 1.5rem;

  /* ── Shadows (slate-900 base) ── */
  --shadow-sm: 0 1px 2px rgba(15, 23, 42, 0.04);
  --shadow-md: 0 1px 3px rgba(15, 23, 42, 0.04), 0 4px 16px rgba(15, 23, 42, 0.06);
  --shadow-lg: 0 1px 3px rgba(15, 23, 42, 0.04), 0 12px 40px rgba(15, 23, 42, 0.10);
  --shadow-glow: 0 0 32px var(--color-accent-glow);

  /* ── Transitions ── */
  --transition-fast: 150ms cubic-bezier(0.4, 0, 0.2, 1);
  --transition-base: 200ms cubic-bezier(0.4, 0, 0.2, 1);
  --transition-slow: 300ms cubic-bezier(0.4, 0, 0.2, 1);
}
```

- [ ] **Step 2: Replace light theme comment block**

Replace the header comment block (lines 1-10) with:

```css
/* ═══════════════════════════════════════════════════════════════════
   PicHost Design System — Theme Tokens

   Light theme is the default (:root).
   Dark theme is activated via .dark class on <html>.

   Design: Professional gallery aesthetic with indigo accent.
   Apple-style glassmorphism — tinted layered glass,
   per-layer blur/opacity, inset highlight lines.
   ═══════════════════════════════════════════════════════════════════ */
```

- [ ] **Step 3: Commit light theme changes**

```bash
git add web-ui/src/theme.css
git commit -m "feat(ui): replace light theme with slate-indigo palette and layered glass tokens"
```

---

### Task 2: Rewrite theme.css — Dark Theme Tokens

**Files:**
- Modify: `web-ui/src/theme.css`

**Interfaces:**
- Consumes: Layer token naming convention from Task 1
- Produces: `.dark` overrides for all color/text/glass/shadow tokens (consumed by Task 3 index.css and all components)

- [ ] **Step 1: Replace dark theme (.dark) block with slate-indigo dark palette + glass layer overrides**

Replace the entire `.dark { ... }` block (lines 96-163) with:

```css
/* ── Dark Theme ─────────────────────────────── */
.dark {
  /* ── Core Backgrounds ── */
  --color-bg: #020617;
  --color-bg-subtle: #0f172a;
  --color-surface: rgba(255, 255, 255, 0.03);
  --color-surface-hover: rgba(255, 255, 255, 0.06);
  --color-surface-elevated: rgba(255, 255, 255, 0.05);
  --color-surface-glass: rgba(255, 255, 255, 0.03);

  /* ── Glassmorphism — Overrides (Dark) ── */
  --glass-highlight: rgba(255, 255, 255, 0.06);
  --glass-tint-base: oklch(0.12 0.005 260);
  --glass-border-base: rgba(255, 255, 255, 0.06);
  --glass-border-strong: rgba(255, 255, 255, 0.10);
  --glass-layer-card-opacity: 0.06;
  --glass-layer-elevated-opacity: 0.08;
  --glass-layer-nav-opacity: 0.10;
  --glass-layer-modal-opacity: 0.12;
  --glass-layer-card-blur: 20px;
  --glass-layer-elevated-blur: 28px;
  --glass-layer-nav-blur: 36px;
  --glass-layer-modal-blur: 44px;
  --glass-layer-card-saturate: 130%;
  --glass-layer-elevated-saturate: 140%;
  --glass-layer-nav-saturate: 150%;
  --glass-layer-modal-saturate: 140%;

  --glass-shadow:
    0 8px 32px rgba(0, 0, 0, 0.45),
    0 1px 3px rgba(0, 0, 0, 0.25);
  --glass-shadow-sm:
    0 2px 8px rgba(0, 0, 0, 0.30),
    0 1px 2px rgba(0, 0, 0, 0.20);

  /* ── Borders ── */
  --color-border: rgba(255, 255, 255, 0.06);
  --color-border-hover: rgba(255, 255, 255, 0.12);
  --color-border-strong: rgba(255, 255, 255, 0.08);

  /* ── Text — Slate scale (light on dark) ── */
  --color-text-primary: #f1f5f9;
  --color-text-secondary: #94a3b8;
  --color-text-muted: #64748b;
  --color-text-on-accent: #ffffff;

  /* ── Accent — Indigo (brighter for dark) ── */
  --color-accent: #818cf8;
  --color-accent-hover: #6366f1;
  --color-accent-active: #4f46e5;
  --color-accent-subtle: rgba(129, 140, 248, 0.10);
  --color-accent-glow: rgba(129, 140, 248, 0.16);
  --color-accent-strong: rgba(129, 140, 248, 0.25);

  --color-accent-gradient: linear-gradient(135deg, #6366f1 0%, #818cf8 50%, #6366f1 100%);
  --color-accent-gradient-hover: linear-gradient(135deg, #4f46e5 0%, #a5b4fc 50%, #4f46e5 100%);

  /* ── Semantic Colors ── */
  --color-danger: #fca5a5;
  --color-danger-hover: #f87171;
  --color-danger-active: #ef4444;
  --color-danger-subtle: rgba(252, 165, 165, 0.08);
  --color-danger-border: rgba(252, 165, 165, 0.15);

  --color-success: #86efac;
  --color-success-hover: #4ade80;
  --color-success-active: #22c55e;
  --color-success-subtle: rgba(134, 239, 172, 0.08);

  --color-warning: #fde68a;
  --color-warning-hover: #fbbf24;
  --color-warning-subtle: rgba(253, 230, 138, 0.08);

  /* ── Shadows (Dark) ── */
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.20);
  --shadow-md: 0 4px 16px rgba(0, 0, 0, 0.30);
  --shadow-lg: 0 8px 40px rgba(0, 0, 0, 0.45);
  --shadow-glow: 0 0 24px var(--color-accent-glow);
}
```

- [ ] **Step 2: Verify theme.css file integrity**

Run: `cd web-ui && npx tsc --noEmit`
Expected: PASS (no type errors — CSS changes don't affect TypeScript)
Check the file has exactly two blocks: `:root { ... }` and `.dark { ... }`, no syntax errors.

- [ ] **Step 3: Commit dark theme changes**

```bash
git add web-ui/src/theme.css
git commit -m "feat(ui): replace dark theme with slate-indigo palette and layered glass overrides"
```

---

### Task 3: Update index.css — Glass Utility Classes + Ambient Background

**Files:**
- Modify: `web-ui/src/index.css`

**Interfaces:**
- Consumes: All `--glass-layer-*`, `--glass-tint-base`, `--glass-highlight`, `--glass-border-base`, `--glass-border-strong`, `--color-accent`, `--color-accent-subtle` from Tasks 1-2
- Produces: `.glass`, `.glass-static`, `.glass-elevated`, `.glass-nav`, `.glass-modal` with tinted layered glass; `.input-field` updated

- [ ] **Step 1: Update ambient background gradients from teal to indigo**

Replace the `body::before` block (lines 81-89) and `.dark body::before` block (lines 93-98):

```css
  /* Clean, calm ambient light — subtle indigo depth for light mode. */
  body::before {
    content: '';
    position: fixed;
    inset: 0;
    z-index: -1;
    pointer-events: none;
    background:
      radial-gradient(ellipse 60% 50% at 15% 5%, rgba(79, 70, 229, 0.03) 0%, transparent 70%),
      radial-gradient(ellipse 40% 40% at 85% 90%, rgba(99, 102, 241, 0.02) 0%, transparent 70%);
  }

  /* Dark mode: subtle indigo ambient for depth without dominating */
  .dark body::before {
    background:
      radial-gradient(ellipse 60% 50% at 20% 10%, rgba(99, 102, 241, 0.04) 0%, transparent 70%),
      radial-gradient(ellipse 50% 40% at 80% 80%, rgba(129, 140, 248, 0.03) 0%, transparent 70%),
      radial-gradient(ellipse 40% 50% at 50% 50%, rgba(99, 102, 241, 0.02) 0%, transparent 70%);
  }
```

- [ ] **Step 2: Update .glass class with tinted card-layer glass**

Replace `.glass` and `.glass:hover` (lines 104-116):

```css
  /* Standard glass card — Layer 1 (card) */
  .glass {
    background: color-mix(in oklch, var(--glass-tint-base) calc(var(--glass-layer-card-opacity) * 100%), transparent);
    border: 1px solid var(--glass-border-base);
    backdrop-filter: blur(var(--glass-layer-card-blur)) saturate(var(--glass-layer-card-saturate));
    -webkit-backdrop-filter: blur(var(--glass-layer-card-blur)) saturate(var(--glass-layer-card-saturate));
    border-radius: var(--radius-lg);
    box-shadow:
      inset 0 1px 0 var(--glass-highlight),
      var(--glass-shadow-sm);
    transition: border-color var(--transition-base), box-shadow var(--transition-base);
  }
  .glass:hover {
    border-color: var(--glass-border-strong);
    box-shadow:
      inset 0 1px 0 var(--glass-highlight),
      var(--glass-shadow);
  }
```

- [ ] **Step 3: Update .glass-static with tinted card-layer glass**

Replace `.glass-static` (lines 119-126):

```css
  /* Glass card without hover lift — Layer 1 (card) */
  .glass-static {
    background: color-mix(in oklch, var(--glass-tint-base) calc(var(--glass-layer-card-opacity) * 100%), transparent);
    border: 1px solid var(--glass-border-base);
    backdrop-filter: blur(var(--glass-layer-card-blur)) saturate(var(--glass-layer-card-saturate));
    -webkit-backdrop-filter: blur(var(--glass-layer-card-blur)) saturate(var(--glass-layer-card-saturate));
    border-radius: var(--radius-lg);
    box-shadow:
      inset 0 1px 0 var(--glass-highlight),
      var(--glass-shadow-sm);
  }
```

- [ ] **Step 4: Update .glass-elevated with tinted elevated-layer glass**

Replace `.glass-elevated` (lines 129-139):

```css
  /* Glass surface with top highlight — Layer 2 (elevated) */
  .glass-elevated {
    background: color-mix(in oklch, var(--glass-tint-base) calc(var(--glass-layer-elevated-opacity) * 100%), transparent);
    border: 1px solid var(--glass-border-base);
    border-top-color: var(--glass-border-strong);
    backdrop-filter: blur(var(--glass-layer-elevated-blur)) saturate(var(--glass-layer-elevated-saturate));
    -webkit-backdrop-filter: blur(var(--glass-layer-elevated-blur)) saturate(var(--glass-layer-elevated-saturate));
    border-radius: var(--radius-lg);
    box-shadow:
      inset 0 1px 0 var(--glass-highlight),
      var(--glass-shadow);
  }
```

- [ ] **Step 5: Update .glass-nav with tinted nav-layer glass**

Replace `.glass-nav` (lines 142-147):

```css
  /* Glass navbar — Layer 3 (nav), bottom border only */
  .glass-nav {
    background: color-mix(in oklch, var(--glass-tint-base) calc(var(--glass-layer-nav-opacity) * 100%), transparent);
    border-bottom: 1px solid var(--glass-border-base);
    backdrop-filter: blur(var(--glass-layer-nav-blur)) saturate(var(--glass-layer-nav-saturate));
    -webkit-backdrop-filter: blur(var(--glass-layer-nav-blur)) saturate(var(--glass-layer-nav-saturate));
    box-shadow: inset 0 1px 0 var(--glass-highlight);
  }
```

- [ ] **Step 6: Update .glass-modal with tinted modal-layer glass**

Replace `.glass-modal` (lines 150-160):

```css
  /* Glass dialog/modal overlay — Layer 4 (modal), deepest blur */
  .glass-modal {
    background: color-mix(in oklch, var(--glass-tint-base) calc(var(--glass-layer-modal-opacity) * 100%), transparent);
    border: 1px solid var(--glass-border-base);
    border-top-color: var(--glass-border-strong);
    backdrop-filter: blur(var(--glass-layer-modal-blur)) saturate(var(--glass-layer-modal-saturate));
    -webkit-backdrop-filter: blur(var(--glass-layer-modal-blur)) saturate(var(--glass-layer-modal-saturate));
    border-radius: var(--radius-xl);
    box-shadow:
      inset 0 1px 0 var(--glass-highlight),
      var(--shadow-lg);
  }
```

- [ ] **Step 7: Update .input-field with card-layer glass + highlight**

Replace `.input-field` background and border (lines 232-234):

Change these three lines:
```css
    background: var(--glass-bg);
    border: 1px solid var(--glass-border);
```

To:
```css
    background: color-mix(in oklch, var(--glass-tint-base) calc(var(--glass-layer-card-opacity) * 100%), transparent);
    border: 1px solid var(--glass-border-base);
```

And update the blur reference (line 235) from `var(--glass-blur)` to `var(--glass-layer-card-blur)`:

```css
    backdrop-filter: blur(var(--glass-layer-card-blur));
    -webkit-backdrop-filter: blur(var(--glass-layer-card-blur));
```

- [ ] **Step 8: Update .badge with new border token**

Replace `.badge` border (line 311) from `var(--glass-border)` to `var(--glass-border-base)`:

```css
    border: 1px solid var(--glass-border-base);
```

- [ ] **Step 9: Verify build**

Run: `cd web-ui && npm run build`
Expected: PASS — `tsc -b && vite build` exits 0, no CSS warnings

- [ ] **Step 10: Commit**

```bash
git add web-ui/src/index.css
git commit -m "feat(ui): update glass classes with Apple-style tinted layered glass and indigo ambient background"
```

---

### Task 4: Fix Login.tsx — Replace Purple Gradient with Accent Token Gradient

**Files:**
- Modify: `web-ui/src/pages/Login.tsx:31-38`

**Interfaces:**
- Consumes: `--color-accent`, `--color-accent-hover` from theme.css

- [ ] **Step 1: Replace the hardcoded purple gradient in Login.tsx**

Replace lines 31-38:
```tsx
          <h1
            className="text-4xl font-bold"
            style={{
              background: 'linear-gradient(135deg, #a5b4fc 0%, #c084fc 50%, #a5b4fc 100%)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
              fontFamily: "'Outfit', system-ui, sans-serif",
            }}
          >
```

With:
```tsx
          <h1
            className="text-4xl font-bold"
            style={{
              background: 'var(--color-accent-gradient)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
              fontFamily: "'Outfit', system-ui, sans-serif",
            }}
          >
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/pages/Login.tsx
git commit -m "fix(ui): replace Login purple gradient with accent token gradient"
```

---

### Task 5: Fix Register.tsx — Replace Purple Gradient with Accent Token Gradient

**Files:**
- Modify: `web-ui/src/pages/Register.tsx:36-43`

**Interfaces:**
- Consumes: `--color-accent-gradient` from theme.css

- [ ] **Step 1: Replace the hardcoded purple gradient in Register.tsx**

Replace lines 36-43:
```tsx
          <h1
            className="text-4xl font-bold"
            style={{
              background: 'linear-gradient(135deg, #a5b4fc 0%, #c084fc 50%, #a5b4fc 100%)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
              fontFamily: "'Outfit', system-ui, sans-serif",
            }}
          >
```

With:
```tsx
          <h1
            className="text-4xl font-bold"
            style={{
              background: 'var(--color-accent-gradient)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
              fontFamily: "'Outfit', system-ui, sans-serif",
            }}
          >
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/pages/Register.tsx
git commit -m "fix(ui): replace Register purple gradient with accent token gradient"
```

---

### Task 6: Fix PreprocessingSettings.tsx — Replace Hardcoded Tailwind Colors with Design Tokens

**Files:**
- Modify: `web-ui/src/components/PreprocessingSettings.tsx`

**Interfaces:**
- Consumes: `--color-accent`, `--color-accent-subtle`, `--color-danger` from theme.css

- [ ] **Step 1: Replace `text-muted-foreground` with token-based color**

Replace ALL occurrences of `text-muted-foreground` with inline style `style={{ color: 'var(--color-text-muted)' }}`:

Lines 17, 25, 50, 59, 68, 98, 125 — everywhere `text-muted-foreground` appears as a className:

```tsx
// Before (example line 17):
<p className="text-sm text-muted-foreground">

// After:
<p className="text-sm" style={{ color: 'var(--color-text-muted)' }}>
```

Apply the same replacement to every `text-muted-foreground` occurrence (6 total: lines 17, 25, 50, 59, 68, 98, 125).

NOTE: `text-muted-foreground` is a shadcn/ui convention class that doesn't exist in this project's Tailwind 4 config, so it resolves to nothing — replace it with the explicit token.

- [ ] **Step 2: Replace rotation button hardcoded colors**

Replace lines 158-160:
```tsx
                className={`px-3 py-1 rounded text-sm border transition-colors ${
                  store.rotate.degrees === deg
                    ? 'border-blue-500 bg-blue-500/20 text-blue-400'
                    : 'border-[var(--color-border)] hover:border-blue-500/50'
                }`}
```

With:
```tsx
                className={`px-3 py-1 rounded text-sm border transition-colors ${
                  store.rotate.degrees === deg
                    ? 'border-[var(--color-accent)] bg-[var(--color-accent-subtle)]'
                    : 'border-[var(--color-border)] hover:border-[var(--color-accent-subtle)]'
                }`}
                style={store.rotate.degrees === deg ? { color: 'var(--color-accent)' } : undefined}
```

- [ ] **Step 3: Replace reset button hardcoded red**

Replace line 175:
```tsx
          className="text-sm text-red-400 hover:text-red-300 transition-colors"
```

With:
```tsx
          className="text-sm transition-colors"
          style={{ color: 'var(--color-danger)' }}
```

Note: After change, the hover state loses its lighter tint. Add hover style:
```tsx
          style={{ color: 'var(--color-danger)' }}
          onMouseEnter={(e) => (e.currentTarget.style.color = 'var(--color-danger-hover)')}
          onMouseLeave={(e) => (e.currentTarget.style.color = 'var(--color-danger)')}
```

- [ ] **Step 4: Commit**

```bash
git add web-ui/src/components/PreprocessingSettings.tsx
git commit -m "fix(ui): replace hardcoded Tailwind colors with design tokens in PreprocessingSettings"
```

---

### Task 7: Fix PreprocessingStatus.tsx — Replace Hardcoded Blue with Accent Tokens

**Files:**
- Modify: `web-ui/src/components/PreprocessingStatus.tsx`

**Interfaces:**
- Consumes: `--color-accent`, `--color-accent-subtle`, `--color-text-muted` from theme.css

- [ ] **Step 1: Replace `text-muted-foreground`**

Replace line 15:
```tsx
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
```

With:
```tsx
      <div className="flex items-center gap-2 text-xs" style={{ color: 'var(--color-text-muted)' }}>
```

- [ ] **Step 2: Replace `text-blue-400 hover:text-blue-300` (Configure link, line 19)**

Replace:
```tsx
          className="text-blue-400 hover:text-blue-300 underline underline-offset-2"
```

With:
```tsx
          className="underline underline-offset-2"
          style={{ color: 'var(--color-accent)' }}
```

- [ ] **Step 3: Replace blue tag badge colors (lines 44-45)**

Replace:
```tsx
          className="rounded bg-blue-500/10 px-1.5 py-0.5 text-blue-400 border border-blue-500/20"
```

With:
```tsx
          className="rounded px-1.5 py-0.5 border text-xs"
          style={{
            backgroundColor: 'var(--color-accent-subtle)',
            color: 'var(--color-accent)',
            borderColor: 'var(--color-accent-subtle)',
          }}
```

- [ ] **Step 4: Replace the second `text-blue-400` (Configure link with icon, lines 51-54)**

Replace:
```tsx
        className="ml-1 text-blue-400 hover:text-blue-300 underline underline-offset-2 flex items-center gap-1"
```

With:
```tsx
        className="ml-1 underline underline-offset-2 flex items-center gap-1"
        style={{ color: 'var(--color-accent)' }}
```

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/components/PreprocessingStatus.tsx
git commit -m "fix(ui): replace hardcoded blue colors with accent design tokens in PreprocessingStatus"
```

---

### Task 8: Fix AdminStats.tsx — Replace Violet Hardcode with Accent Token

**Files:**
- Modify: `web-ui/src/pages/admin/AdminStats.tsx`

**Interfaces:**
- Consumes: `--color-accent`, `--glass-bg` (replaced by layered tokens in Tasks 1-3), `--glass-blur` (replaced by layered tokens in Tasks 1-3) from theme.css

- [ ] **Step 1: Replace violet icon color (line 39)**

Replace:
```tsx
  { key: 'total_images', label: 'Total Images', icon: ImageIcon, color: '#a78bfa' },
```

With:
```tsx
  { key: 'total_images', label: 'Total Images', icon: ImageIcon, color: 'var(--color-accent)' },
```

- [ ] **Step 2: Replace violet backend bar color (line 122)**

Replace:
```tsx
                    backgroundColor: name === 'local' ? 'var(--color-accent)' : '#a78bfa',
```

With:
```tsx
                    backgroundColor: 'var(--color-accent)',
```

- [ ] **Step 3: Update inline glass style to use new per-layer tokens (stat cards, lines 68-71)**

Replace:
```tsx
              style={{
                backgroundColor: 'var(--glass-bg)',
                border: '1px solid var(--glass-border)',
                backdropFilter: 'blur(var(--glass-blur))',
              }}
```

With:
```tsx
              style={{
                backgroundColor: `color-mix(in oklch, var(--glass-tint-base) ${Number(getComputedStyle(document.documentElement).getPropertyValue('--glass-layer-card-opacity').trim()) * 100}%, transparent)`,
                border: '1px solid var(--glass-border-base)',
                backdropFilter: 'blur(var(--glass-layer-card-blur)) saturate(var(--glass-layer-card-saturate))',
              }}
```

NOTE: Since `color-mix()` with CSS variable opacity cannot be used inline (it requires the `calc()` expression), use a simpler fallback approach — keep the inline style as `backgroundColor: 'var(--glass-bg)'` BUT add a static `--glass-bg` token for backward compatibility in the theme.css, or better: add a `.glass` class to these cards instead of inline styles.

**Better approach — add `.glass` class to cards:**

Replace the inline style card block (lines 65-72) with a `.glass` class:
```tsx
            <div key={key} className="glass rounded-xl p-4">
```

Then remove the `style={{}}` block entirely for those stat cards — they now inherit `.glass` tinted background, border, blur, highlight, and shadow from index.css.

Also update the backend breakdown panel (lines 90-96) to use `.glass`:
```tsx
      <div className="glass mt-6 rounded-xl p-4">
```

Remove the inline `style={{}}` block for it as well.

- [ ] **Step 4: Commit**

```bash
git add web-ui/src/pages/admin/AdminStats.tsx
git commit -m "fix(ui): replace hardcoded violet and inline glass styles with design tokens and .glass class in AdminStats"
```

---

### Task 9: Fix ProtectedRoute.tsx — Replace Hardcoded Tailwind Gray with Token

**Files:**
- Modify: `web-ui/src/components/ProtectedRoute.tsx:11-13`

**Interfaces:**
- Consumes: `--color-bg`, `--color-text-primary` from theme.css

- [ ] **Step 1: Replace hardcoded spinner background and border colors**

Replace line 11:
```tsx
      <div className="flex min-h-screen items-center justify-center bg-gray-950">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-gray-600 border-t-white" />
```

With:
```tsx
      <div className="flex min-h-screen items-center justify-center" style={{ backgroundColor: 'var(--color-bg)' }}>
        <div
          className="h-8 w-8 animate-spin rounded-full border-2"
          style={{
            borderColor: 'var(--color-border-strong)',
            borderTopColor: 'var(--color-text-primary)',
          }}
        />
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/components/ProtectedRoute.tsx
git commit -m "fix(ui): replace hardcoded Tailwind gray with design tokens in ProtectedRoute spinner"
```

---

### Task 10: Final Build Verification

- [ ] **Step 1: Run the full build**

```bash
cd web-ui && npm run build
```

Expected: `tsc -b && vite build` exits 0 with no errors and no warnings.

- [ ] **Step 2: Verify all changed files are committed**

```bash
git status
```

Expected: clean working tree.

- [ ] **Step 3: Final commit if needed**

If any uncommitted changes remain from verification, commit them.

---

### Task 11: Visual QA Checklist

After build passes, manually verify these pages in both light and dark modes:

- [ ] **Login page** — Logo uses indigo gradient (not purple), form card has tinted glass with top highlight line
- [ ] **Register page** — Same as Login
- [ ] **Dashboard** — DropZone glass has highlight line, UploadCard glass is tinted, quota bar uses accent color
- [ ] **Gallery** — Image tiles have tinted glass with highlight, sidebar glass is more opaque than tile glass (layered depth visible), selection toolbar uses accent
- [ ] **ImageDetail** — Preview and info panels have `.glass-elevated` tinted appearance
- [ ] **Settings** — Section nav glass + content panels glass, Preprocessing rotation buttons use accent color, reset button uses danger color
- [ ] **Admin** — Tab bar glass, stat cards use `.glass` class, backend breakdown bar uses uniform accent color
- [ ] **NavBar** — Sticky glass-nav is most opaque layer, distinct from card glass below
- [ ] **Theme toggle** — Switching light ↔ dark works, colors transition correctly
- [ ] **Glass depth hierarchy** — NavBar > Modal > Sidebar/Dropdown > Card (most transparent to least transparent)

---

## Completion Checklist

- [ ] `npm run build` passes with zero errors
- [ ] All 9 files committed with descriptive messages
- [ ] Light mode: slate-indigo palette visible, no stone/teal/warm tones remaining
- [ ] Dark mode: slate-indigo palette visible, no old cyan/teal tones remaining
- [ ] Glass layers: 4 distinct opacity/blur levels visible (NavBar most opaque, cards most transparent)
- [ ] Glass tint: cards have subtle cool-white tint (not pure white)
- [ ] Glass highlight: all glass surfaces have `inset 0 1px 0` top highlight line
- [ ] No hardcoded Tailwind colors remain in changed components (`blue-*`, `red-*`, `gray-*`, `#a78bfa`)
- [ ] Brand gradient unified across NavBar + Login + Register
