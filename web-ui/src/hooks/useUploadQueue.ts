import { useState, useRef, useEffect, useCallback } from 'react'
import { uploadImage, type UploadResult } from '../api/client'

export type UploadStatus = 'pending' | 'uploading' | 'done' | 'error'

export interface UploadTask {
  id: string
  file: File
  status: UploadStatus
  progress: number // 0-100
  result: UploadResult | null
  error: string | null
  storageConfigIds?: string[]
}

const MAX_CONCURRENT = 3

function makeId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
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
    (files: File[], storageConfigIds?: string[]) => {
      if (files.length === 0) return
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
            storageConfigIds,
          })
        }
        return next
      })
      pendingRef.current.push(...ids)
      // Kick off processing after the state update queued
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
