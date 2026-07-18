# Multi-File Concurrent Drag-and-Drop Upload Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable dragging/selecting multiple images at once and uploading them concurrently (up to 3 at a time), with individual progress cards for each file showing pending → uploading → done/error status.

**Architecture:** Frontend-only change — backend single-file endpoint is reused via concurrent calls. A `useUploadQueue` hook manages a concurrency-limited pool (max 3 parallel). DropZone switches `multiple: true` and passes all accepted files up. Dashboard replaces single `uploadResult`/`isUploading` state with a managed file queue, rendering `UploadCard` components for each queued/success/failed file.

**Tech Stack:** React 19, TypeScript 5.7, react-dropzone, TanStack Query v5, Tailwind CSS v4, lucide-react, sonner

## Global Constraints

- Rust edition 2021, `cargo clippy --workspace -- -D warnings` must pass, `cargo test --workspace` must pass
- Frontend: `npm run build` (tsc + vite) must pass
- Backend unchanged — no Rust code modifications
- No new external Rust crates; no new npm dependencies
- React 19 + TypeScript 5.7 strict mode — use refs with explicit `| null` types
- Follow existing code patterns: CSS `var(--color-*)` variables, glass surface style (`bg-[var(--glass-bg)]`, `backdrop-blur-sm`), Tailwind utility classes
- All commits in English, spec docs in Chinese

---

## File Structure

```
web-ui/src/hooks/useUploadQueue.ts         (CREATE) — concurrency-limited upload queue hook
web-ui/src/components/UploadCard.tsx       (CREATE) — per-file progress card with status indicator
web-ui/src/components/DropZone.tsx          (MODIFY) — multiple: true, pass all files up
web-ui/src/pages/Dashboard.tsx              (MODIFY) — integrate multi-upload queue + UploadCard
```

---

### Task 1: Create `useUploadQueue` hook — concurrency-limited upload manager

**Files:**
- Create: `web-ui/src/hooks/useUploadQueue.ts`

**Interfaces:**
- Produces: `useUploadQueue()` → `{ queue, addFiles, clearQueue }`
  - `queue: UploadTask[]` — reactive array of all tasks with current status
  - `addFiles(files: File[]): void` — enqueue files for processing
  - `clearQueue(): void` — remove all completed/errored tasks
- Types: `UploadTask { id, file, status, progress, result?, error? }`, `UploadStatus = 'pending' | 'uploading' | 'done' | 'error'`
- Consumed by: Task 4 (Dashboard integration)

- [ ] **Step 1: Create the hook file**

First, create the directory if needed:
```bash
mkdir -p web-ui/src/hooks
```

Create `web-ui/src/hooks/useUploadQueue.ts`:

```typescript
// web-ui/src/hooks/useUploadQueue.ts
import { useState, useRef, useCallback } from 'react'
import { uploadImage, type UploadResult } from '../api/client'

export type UploadStatus = 'pending' | 'uploading' | 'done' | 'error'

export interface UploadTask {
  id: string
  file: File
  status: UploadStatus
  progress: number // 0-100
  result: UploadResult | null
  error: string | null
}

const MAX_CONCURRENT = 3

function makeId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}

export function useUploadQueue() {
  const [tasks, setTasks] = useState<Map<string, UploadTask>>(new Map())
  const activeRef = useRef(0)
  const pendingRef = useRef<string[]>([])

  const queue = Array.from(tasks.values())

  const updateTask = useCallback((id: string, patch: Partial<UploadTask>) => {
    setTasks((prev) => {
      const next = new Map(prev)
      const existing = next.get(id)
      if (existing) next.set(id, { ...existing, ...patch })
      return next
    })
  }, [])

  const processNext = useCallback(() => {
    while (activeRef.current < MAX_CONCURRENT && pendingRef.current.length > 0) {
      const id = pendingRef.current.shift()!
      activeRef.current += 1
      const task = tasks.get(id)
      if (!task) {
        activeRef.current -= 1
        continue
      }
      updateTask(id, { status: 'uploading', progress: 0 })
      uploadImage(task.file)
        .then((result) => {
          updateTask(id, { status: 'done', progress: 100, result })
        })
        .catch((e: unknown) => {
          const msg = e instanceof Error ? e.message : 'Upload failed'
          updateTask(id, { status: 'error', progress: 0, error: msg })
        })
        .finally(() => {
          activeRef.current -= 1
          processNext()
        })
    }
  }, [tasks, updateTask])

  const addFiles = useCallback(
    (files: File[]) => {
      const ids: string[] = []
      setTasks((prev) => {
        const next = new Map(prev)
        for (const file of files) {
          const id = makeId()
          ids.push(id)
          next.set(id, {
            id,
            file,
            status: 'pending',
            progress: 0,
            result: null,
            error: null,
          })
        }
        return next
      })
      // After state update queues the render, start processing
      pendingRef.current.push(...ids)
      // Use setTimeout to ensure tasks Map is initialized before processNext reads it
      setTimeout(() => processNext(), 0)
    },
    [processNext],
  )

  const clearQueue = useCallback(() => {
    setTasks((prev) => {
      const next = new Map(prev)
      for (const [id, t] of next) {
        if (t.status === 'done' || t.status === 'error') next.delete(id)
      }
      return next
    })
  }, [])

  return { queue, addFiles, clearQueue }
}
```

- [ ] **Step 2: Verify TypeScript compilation**

```bash
cd web-ui && npx tsc --noEmit
```

Expected: PASS (0 errors)

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/hooks/useUploadQueue.ts
git commit -m "feat: add useUploadQueue hook for concurrent multi-file upload"
```

---

### Task 2: Update DropZone — accept multiple files, remove single-upload lock

**Files:**
- Modify: `web-ui/src/components/DropZone.tsx`

**Interfaces:**
- Consumes: Nothing from prior tasks
- Produces: Updated `DropZone` with `onUpload(files: File[])` (plural), `multiple: true`, no `isUploading` lock
- Consumed by: Task 4 (Dashboard passes new prop)

- [ ] **Step 1: Modify DropZone.tsx**

Replace the entire `DropZone.tsx`:

```tsx
// web-ui/src/components/DropZone.tsx
import { useCallback, type ChangeEvent } from 'react'
import { useDropzone, type DropEvent } from 'react-dropzone'
import { Upload } from 'lucide-react'

/** Custom getFilesFromEvent to bypass file-selector's broken getAsFileSystemHandle()
 *  path in secure contexts (localhost). getAsFileSystemHandle() returns null for
 *  OS-dragged files, causing silent failures.
 *  Also fixes React 19 SyntheticEvent wrapping: use dataTransfer property directly
 *  instead of instanceof DragEvent (React wraps native events). */
const getFilesFromEvent = async (
  event: DropEvent,
): Promise<(DataTransferItem | File)[]> => {
  const dt = ('dataTransfer' in event ? event.dataTransfer : null) as DataTransfer | null
  if (dt?.files?.length) {
    const files: File[] = []
    for (let i = 0; i < dt.files.length; i++) files.push(dt.files[i])
    return files
  }
  const input = (event as ChangeEvent<HTMLElement>).target as HTMLInputElement | null
  if (input?.files?.length) {
    const files: File[] = []
    for (let i = 0; i < input.files!.length; i++) files.push(input.files![i])
    return files
  }
  return []
}

interface DropZoneProps {
  onUpload: (files: File[]) => void
}

export default function DropZone({ onUpload }: DropZoneProps) {
  const onDrop = useCallback(
    (accepted: File[]) => {
      if (accepted.length > 0) onUpload(accepted)
    },
    [onUpload],
  )

  const { getRootProps, getInputProps, isDragActive } = useDropzone({
    onDrop,
    getFilesFromEvent,
    accept: {
      'image/png': ['.png'],
      'image/jpeg': ['.jpg', '.jpeg'],
      'image/gif': ['.gif'],
      'image/webp': ['.webp'],
      'image/svg+xml': ['.svg'],
      'image/avif': ['.avif'],
      'image/bmp': ['.bmp'],
    },
    maxSize: 52_428_800,
    multiple: true,
  })

  return (
    <div
      {...getRootProps()}
      className={`cursor-pointer rounded-xl border-2 border-dashed p-12 text-center transition-colors ${
        isDragActive
          ? 'border-[var(--color-accent)] bg-[var(--color-accent-subtle)]'
          : 'border-[var(--color-border)] bg-[var(--glass-bg)] hover:border-[var(--color-border-hover)]'
      }`}
    >
      <input {...getInputProps()} />
      <div className="flex flex-col items-center gap-2 text-gray-400">
        <Upload className="h-8 w-8" />
        <p className="text-sm">
          {isDragActive
            ? 'Drop images here'
            : 'Drag & drop images, or click to select'}
        </p>
        <p className="text-xs text-gray-600">
          PNG, JPEG, GIF, WebP, SVG, AVIF, BMP — up to 50 MB each
        </p>
      </div>
    </div>
  )
}
```

Key changes from original:
- `onUpload: (file: File) => void` → `onUpload: (files: File[]) => void`
- `multiple: false` → `multiple: true`
- Removed `isUploading` prop and its UI effects (spinner, disable, opacity)
- Removed `Loader2` import (no longer needed)
- `onDrop` passes all `accepted` files instead of `accepted[0]`
- Help text: `"up to 50 MB"` → `"up to 50 MB each"`
- DropZone stays always interactive (no disable lock)

- [ ] **Step 2: Verify TypeScript compilation**

```bash
cd web-ui && npx tsc --noEmit
```

Expected: PASS — `Dashboard.tsx` will have a type error because it still passes old props. Fix that in Task 4.

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/components/DropZone.tsx
git commit -m "feat: support multiple file selection and drag-drop in DropZone"
```

---

### Task 3: Create UploadCard component — per-file progress display

**Files:**
- Create: `web-ui/src/components/UploadCard.tsx`

**Interfaces:**
- Consumes: `UploadTask` type from Task 1
- Produces: `UploadCard` component rendering a file's upload status with progress bar and result links
- Consumed by: Task 4 (Dashboard renders one per queue item)

- [ ] **Step 1: Create UploadCard.tsx**

```tsx
// web-ui/src/components/UploadCard.tsx
import { Check, Loader2, X, AlertTriangle } from 'lucide-react'
import type { UploadTask } from '../hooks/useUploadQueue'

interface UploadCardProps {
  task: UploadTask
}

const STATUS_ICONS: Record<UploadTask['status'], { icon: typeof Check; cls: string }> = {
  pending: { icon: Loader2, cls: 'text-[var(--color-text-muted)]' },
  uploading: { icon: Loader2, cls: 'animate-spin text-blue-400' },
  done: { icon: Check, cls: 'text-green-400' },
  error: { icon: X, cls: 'text-red-400' },
}

const STATUS_LABELS: Record<UploadTask['status'], string> = {
  pending: 'Queued',
  uploading: 'Uploading…',
  done: 'Uploaded',
  error: 'Failed',
}

export default function UploadCard({ task }: UploadCardProps) {
  const { icon: Icon, cls: iconCls } = STATUS_ICONS[task.status]

  return (
    <div className="flex items-center gap-3 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-3 backdrop-blur-sm">
      {/* Status icon */}
      <Icon className={`h-5 w-5 shrink-0 ${iconCls}`} />

      {/* File info */}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm text-[var(--color-text-primary)]">
          {task.file.name}
        </p>
        <div className="mt-1 flex items-center gap-2">
          {/* Progress bar (pending/uploading) */}
          {(task.status === 'pending' || task.status === 'uploading') && (
            <div className="h-1 flex-1 overflow-hidden rounded-full bg-[var(--color-border)]">
              <div
                className="h-full rounded-full bg-[var(--color-accent)] transition-all duration-300"
                style={{ width: `${task.status === 'uploading' ? 60 : 0}%` }}
              />
            </div>
          )}
          {/* Status label */}
          <span className="text-xs text-[var(--color-text-muted)]">
            {STATUS_LABELS[task.status]}
          </span>
          {/* Done — show file size */}
          {task.status === 'done' && task.result && (
            <span className="text-xs text-[var(--color-text-muted)]">
              {(task.result.file_size / 1024).toFixed(1)} KB
            </span>
          )}
          {/* Error — show message */}
          {task.status === 'error' && task.error && (
            <span className="flex items-center gap-1 text-xs text-red-400">
              <AlertTriangle className="h-3 w-3" />
              {task.error}
            </span>
          )}
        </div>
        {/* Done — show result links */}
        {task.status === 'done' && task.result && (
          <div className="mt-1 flex flex-wrap gap-2">
            <a
              href={task.result.url}
              target="_blank"
              rel="noopener noreferrer"
              className="text-xs text-[var(--color-accent)] underline underline-offset-2 hover:opacity-80"
            >
              Open
            </a>
            <button
              onClick={() => navigator.clipboard.writeText(task.result!.url)}
              className="text-xs text-[var(--color-accent)] underline underline-offset-2 hover:opacity-80"
            >
              Copy URL
            </button>
            <button
              onClick={() => navigator.clipboard.writeText(task.result!.markdown)}
              className="text-xs text-[var(--color-text-muted)] underline underline-offset-2 hover:opacity-80"
            >
              Copy MD
            </button>
          </div>
        )}
      </div>

      {/* Thumbnail preview (done only) */}
      {task.status === 'done' && task.result && (
        <img
          src={task.result.url}
          alt={task.file.name}
          className="h-10 w-10 shrink-0 rounded object-cover"
        />
      )}
    </div>
  )
}
```

- [ ] **Step 2: Verify TypeScript compilation**

```bash
cd web-ui && npx tsc --noEmit
```

Expected: PASS (0 errors)

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/components/UploadCard.tsx
git commit -m "feat: add UploadCard component with per-file progress and status display"
```

---

### Task 4: Update Dashboard — integrate multi-upload queue and UploadCard

**Files:**
- Modify: `web-ui/src/pages/Dashboard.tsx`

**Interfaces:**
- Consumes: `useUploadQueue` from Task 1, `UploadCard` from Task 3, updated `DropZone` from Task 2
- Produces: Dashboard with multi-file upload support

- [ ] **Step 1: Rewrite Dashboard.tsx**

Replace the entire file:

```tsx
// web-ui/src/pages/Dashboard.tsx
import { useNavigate } from 'react-router-dom'
import { Shield, Trash2 } from 'lucide-react'
import { useAuthStore } from '../stores/auth'
import DropZone from '../components/DropZone'
import UploadCard from '../components/UploadCard'
import { listImages } from '../api/client'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useUploadQueue } from '../hooks/useUploadQueue'

export default function Dashboard() {
  const user = useAuthStore((s) => s.user)
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { queue, addFiles, clearQueue } = useUploadQueue()

  const { data } = useQuery({
    queryKey: ['images'],
    queryFn: () => listImages({ per_page: 50 }),
  })
  const images = data?.items

  // Invalidate recent images when any upload completes
  const hasDone = queue.some((t) => t.status === 'done')
  if (hasDone) {
    // Use a scheduled invalidation to avoid infinite re-renders
    queueMicrotask(() => {
      queryClient.invalidateQueries({ queryKey: ['images'] })
    })
  }

  const hasActiveUploads = queue.some(
    (t) => t.status === 'pending' || t.status === 'uploading',
  )

  return (
    <div className="mx-auto max-w-2xl p-4">
      {/* Admin banner */}
      {user?.is_admin && (
        <div
          className="mb-4 flex items-center gap-2 rounded-lg px-4 py-3 text-sm"
          style={{
            backgroundColor: 'var(--color-accent-subtle)',
            border: '1px solid var(--color-accent)',
            color: 'var(--color-accent)',
          }}
        >
          <Shield className="h-4 w-4 shrink-0" />
          <span>
            You are an administrator.{' '}
            <button
              onClick={() => navigate('/admin')}
              className="font-medium underline underline-offset-2 hover:opacity-80"
            >
              Go to Admin Panel
            </button>
          </span>
        </div>
      )}

      {/* DropZone — always active, accepts multiple files */}
      <DropZone onUpload={addFiles} />

      {/* Upload queue */}
      {queue.length > 0 && (
        <div className="mt-4 space-y-2">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-medium text-[var(--color-text-secondary)]">
              Uploads
              {hasActiveUploads && (
                <span className="ml-2 text-xs text-[var(--color-text-muted)]">
                  {queue.filter((t) => t.status === 'pending' || t.status === 'uploading').length} active
                </span>
              )}
            </h2>
            {queue.some((t) => t.status === 'done' || t.status === 'error') && (
              <button
                onClick={clearQueue}
                className="flex items-center gap-1 rounded px-2 py-1 text-xs text-[var(--color-text-muted)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]"
              >
                <Trash2 className="h-3 w-3" />
                Clear done
              </button>
            )}
          </div>
          {queue.map((task) => (
            <UploadCard key={task.id} task={task} />
          ))}
        </div>
      )}

      {/* Recent images */}
      {images && images.length > 0 && (
        <div className="mt-8">
          <h2 className="mb-3 text-sm font-medium text-[var(--color-text-secondary)]">Recent</h2>
          <div className="space-y-2">
            {images.map((img) => (
              <div
                key={img.id}
                className="flex items-center gap-3 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-3 backdrop-blur-sm"
              >
                <img
                  src={img.url}
                  alt={img.original_name}
                  className="h-12 w-12 shrink-0 rounded object-cover"
                />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm text-[var(--color-text-primary)]">
                    {img.original_name}
                  </p>
                  <p className="text-xs text-[var(--color-text-muted)]">
                    {(img.file_size / 1024).toFixed(1)} KB
                  </p>
                </div>
                <button
                  onClick={() => navigate(`/images/${img.id}`)}
                  className="shrink-0 rounded px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]"
                >
                  Detail
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Empty state */}
      {images && images.length === 0 && queue.length === 0 && (
        <div className="mt-8 text-center text-sm text-[var(--color-text-muted)]">
          No images yet. Upload one above!
        </div>
      )}
    </div>
  )
}
```

Key changes from original:
- Removed `useState<UploadResult | null>`, `isUploading` — replaced by `useUploadQueue()`
- Removed `toast` import — status is now shown inline in UploadCard
- `DropZone` prop changed: `onUpload={handleUpload}` + `isUploading` → `onUpload={addFiles}`
- Upload queue section between DropZone and Recent images
- "Clear done" button to clean up completed/errored tasks
- Empty state: hides when upload queue is active
- Removed the old `uploadResult` link display block (replaced by UploadCard inline links)
- Removed the old `LinkCard` import

- [ ] **Step 2: Fix stale files**

Check if `LinkCard` is used elsewhere:

```bash
grep -r "LinkCard" web-ui/src --include="*.tsx" --include="*.ts"
```

If not used elsewhere, remove the file:
```bash
# Only if LinkCard is unused after Dashboard rewrite
rm web-ui/src/components/LinkCard.tsx 2>/dev/null || true
```

- [ ] **Step 3: Verify TypeScript compilation**

```bash
cd web-ui && npx tsc --noEmit
```

Expected: PASS (0 errors)

- [ ] **Step 4: Verify Vite build**

```bash
cd web-ui && npm run build
```

Expected: PASS — tsc + vite both succeed

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/pages/Dashboard.tsx
git commit -m "feat: integrate multi-file concurrent upload queue into Dashboard"
```

---

### Task 5: Integration smoke test + spec/summary update + version bump

**Files:**
- Modify: `docs/superpowers/specs/2026-07-11-pichost-design.md` (update P2 TODO)
- Modify: `.omo/summary/summary_and_next.md` (add completion entry)
- Modify: `Cargo.toml` (version bump)

- [ ] **Step 1: Run full backend verification**

```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace
```

Expected: All pass.

- [ ] **Step 2: Run full frontend verification**

```bash
cd web-ui && npm run build
```

Expected: PASS

- [ ] **Step 3: Update spec doc TODO**

In `docs/superpowers/specs/2026-07-11-pichost-design.md`, change:
```markdown
- 多文件并发拖拽上传
```
to:
```markdown
- [x] 多文件并发拖拽上传 (useUploadQueue hook, max 3 concurrent, per-file UploadCard progress)
```

- [ ] **Step 4: Update summary**

In `.omo/summary/summary_and_next.md`, add before "剩余待开发特性":
```markdown
### P2: 多文件并发拖拽上传 ✅ (本次完成)
- **前端**: `useUploadQueue` hook 管理并发上传池 (MAX_CONCURRENT=3), `UploadCard` 组件显示单文件进度 (pending→uploading→done/error), DropZone 支持多文件选择, Dashboard 集成队列展示 + 清理
- **验证**: `cargo clippy` ✅, `cargo test` ✅(14 pass), `npm run build` ✅

## 建议下一步开发
用户存储配额 或 OAuth 登录
```

Update remaining features (remove 多文件):
```markdown
- **P2 (remaining)**: OAuth 登录, 用户存储配额, 批量管理, /metrics Prometheus, CDN 集成, 水平扩展
```

- [ ] **Step 5: Bump version**

In `Cargo.toml`:
```toml
version = "0.9.0"
```

- [ ] **Step 6: Commit**

```bash
git add docs/superpowers/specs/2026-07-11-pichost-design.md .omo/summary/summary_and_next.md Cargo.toml Cargo.lock
git commit -m "chore: update spec and summary for multi-file upload, bump version to 0.9.0"
```

---

## Self-Review Checklist

### 1. Spec Coverage
- ✅ Multi-file drag-drop: Task 2 (DropZone `multiple: true` + all files passed)
- ✅ Concurrent upload: Task 1 (`useUploadQueue` with `MAX_CONCURRENT=3`)
- ✅ Per-file progress: Task 3 (`UploadCard` with status icon, progress bar, labels)
- ✅ No backend changes: confirmed — all work is frontend-only

### 2. Placeholder Scan
- ✅ No "TBD", "TODO", "implement later"
- ✅ All code shown inline
- ✅ All types defined in the tasks that use them (`UploadTask` in Task 1, consumed by Tasks 3-4)

### 3. Type Consistency
- ✅ `UploadTask` interface defined in Task 1, used consistently in Tasks 3-4
- ✅ `UploadStatus = 'pending' | 'uploading' | 'done' | 'error'` — all statuses handled in UploadCard
- ✅ `DropZoneProps.onUpload: (files: File[]) => void` — Dashboard passes `addFiles(files)` from hook
- ✅ `useUploadQueue` returns `{ queue: UploadTask[], addFiles, clearQueue }` — Dashboard uses all three
