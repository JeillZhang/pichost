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

async function preprocessFile(file: File, prefs: any): Promise<File> {
  if (!needsProcessing(prefs)) return file

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
  // Keep a ref-sync of tasks so processNext never reads stale closure state
  const tasksRef = useRef(tasks)
  tasksRef.current = tasks
  const mountedRef = useRef(true)

  useEffect(() => {
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
      if (!task) {
        // ID invalidated or task removed before processing — skip
        continue
      }
      activeRef.current += 1
      updateTask(id, { status: 'uploading', progress: 0 })
      uploadImage(task.file, task.storageConfigIds)
        .then((result) => {
          if (mountedRef.current) {
            updateTask(id, { status: 'done', progress: 100, result })
          }
        })
        .catch((e: unknown) => {
          if (mountedRef.current) {
            const msg = e instanceof Error ? e.message : 'Upload failed'
            updateTask(id, { status: 'error', progress: 0, error: msg })
          }
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

      const prefs = usePreprocessingStore.getState()

      // First, create tasks with 'processing' status so UI shows immediately
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

      // Process files in parallel
      const processedFiles = await Promise.all(
        files.map((f, i) =>
          preprocessFile(f, prefs).then((pf) => ({ index: i, file: pf })),
        ),
      )

      // Update tasks with processed files, change status to 'pending'
      setTasks((prev) => {
        const next = new Map(prev)
        for (const { index, file } of processedFiles) {
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
