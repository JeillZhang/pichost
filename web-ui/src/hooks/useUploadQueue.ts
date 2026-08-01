import { useState, useRef, useEffect, useCallback } from 'react'
import { uploadImage, type UploadResult } from '../api/client'
import { usePreprocessingStore } from '../stores/preprocessing'
import { needsProcessing } from '../workers/imageProcessor'

export type UploadStatus = 'pending' | 'processing' | 'uploading' | 'done' | 'error'

export interface UploadTask {
  id: string
  file: File
  status: UploadStatus
  progress: number // 0-100
  result: UploadResult | null
  error: string | null
  storageConfigIds?: string[]
  processingStatus?: 'processing' | 'done' | 'failed'
}

const MAX_CONCURRENT = 3

function makeId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}

let _worker: Worker | null = null
function getWorker(): Worker {
  if (!_worker) {
    _worker = new Worker(
      new URL('../workers/imageProcessor.worker.ts', import.meta.url),
      { type: 'module' },
    )
  }
  return _worker
}

let _supportsOffscreenCanvas: boolean | null = null
function supportsOffscreenCanvas(): boolean {
  if (_supportsOffscreenCanvas === null) {
    try {
      _supportsOffscreenCanvas = typeof OffscreenCanvas !== 'undefined'
    } catch {
      _supportsOffscreenCanvas = false
    }
  }
  return _supportsOffscreenCanvas
}

/** Extract only serializable prefs — store state includes setter functions
 *  that break structured clone when sent to a Web Worker. */
function serializablePrefs(raw: ReturnType<typeof usePreprocessingStore.getState>) {
  return {
    stripExif: raw.stripExif,
    resize: { ...raw.resize },
    formatConvert: { ...raw.formatConvert },
    compression: { ...raw.compression },
    rotate: { ...raw.rotate },
  }
}

/** Check whether actual canvas/worker work is needed.  If only stripExif is on
 *  and the file is not JPEG the operation is a no-op — skip the worker. */
function isWorkerNeeded(prefs: ReturnType<typeof serializablePrefs>, file: File): boolean {
  if (!needsProcessing(prefs)) return false
  const canvasNeeded =
    prefs.resize.enabled ||
    prefs.formatConvert.enabled ||
    prefs.compression.enabled ||
    prefs.rotate.enabled
  if (canvasNeeded) return true
  // Only stripExif is on → worker only needed for JPEG
  return prefs.stripExif && file.type === 'image/jpeg'
}

async function preprocessFile(file: File, prefs: ReturnType<typeof serializablePrefs>): Promise<File> {
  if (!isWorkerNeeded(prefs, file)) return file

  if (!supportsOffscreenCanvas()) {
    const { processFile: mainProcess } = await import('../workers/imageProcessor')
    return mainProcess(file, prefs)
  }

  return new Promise((resolve, reject) => {
    const worker = getWorker()
    const handler = (e: MessageEvent) => {
      worker.removeEventListener('message', handler)
      if (e.data.success) {
        resolve(e.data.file)
      } else {
        reject(new Error(e.data.error))
      }
    }
    worker.addEventListener('message', handler)
    worker.postMessage({ file, prefs })
  })
}

export function useUploadQueue() {
  const [tasks, setTasks] = useState<Map<string, UploadTask>>(new Map())
  const activeRef = useRef(0)
  const pendingRef = useRef<string[]>([])
  const tasksRef = useRef(tasks)
  tasksRef.current = tasks

  // Track mount state to avoid scheduling processNext after unmount.
  // State updates (.then/.catch) are NOT gated on this — React 18+
  // safely ignores setState on unmounted components.
  const mountedRef = useRef(true)
  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
    }
  }, [])

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
      const task = tasksRef.current.get(id)
      if (!task) continue

      activeRef.current += 1
      updateTask(id, { status: 'uploading', progress: 0 })

      uploadImage(task.file, task.storageConfigIds)
        .then((result) => {
          updateTask(id, { status: 'done', progress: 100, result })
        })
        .catch((e: unknown) => {
          const msg = e instanceof Error ? e.message : 'Upload failed'
          console.error('[upload] failed:', msg, e)
          updateTask(id, { status: 'error', progress: 0, error: msg })
        })
        .finally(() => {
          activeRef.current -= 1
          if (mountedRef.current) {
            processNext()
          }
        })
    }
  }, [updateTask])

  const addFiles = useCallback(
    async (files: File[], storageConfigIds?: string[]) => {
      if (files.length === 0) return

      const prefs = serializablePrefs(usePreprocessingStore.getState())

      // Create tasks with 'processing' status so UI shows immediately
      const ids: string[] = []
      setTasks((prev) => {
        const next = new Map(prev)
        for (const file of files) {
          const id = makeId()
          ids.push(id)
          next.set(id, {
            id,
            file,
            status: 'processing',
            progress: 0,
            result: null,
            error: null,
            storageConfigIds,
            processingStatus: 'processing',
          })
        }
        return next
      })

      // Preprocess in parallel — errors here leave tasks in 'processing'
      // state which the user can see and retry by re-uploading.
      let processed: { index: number; file: File }[]
      try {
        processed = await Promise.all(
          files.map((f, i) =>
            preprocessFile(f, prefs).then((pf) => ({ index: i, file: pf })),
          ),
        )
      } catch (err) {
        console.error('[preprocess] failed:', err)
        const msg = err instanceof Error ? err.message : 'Preprocessing failed'
        setTasks((prev) => {
          const next = new Map(prev)
          for (const id of ids) {
            const t = next.get(id)
            if (t) next.set(id, { ...t, status: 'error', error: msg, processingStatus: 'failed' })
          }
          return next
        })
        return
      }

      // Update tasks with processed files, move to 'pending'
      setTasks((prev) => {
        const next = new Map(prev)
        for (const { index, file } of processed) {
          const id = ids[index]
          const existing = next.get(id)
          if (existing) {
            next.set(id, {
              ...existing,
              file,
              status: 'pending',
              processingStatus: 'done',
            })
          }
        }
        return next
      })

      pendingRef.current.push(...ids)
      setTimeout(() => {
        if (mountedRef.current) processNext()
      }, 0)
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
