import { useRef, useCallback, useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useInfiniteQuery, keepPreviousData, useQueryClient } from '@tanstack/react-query'
import { listImages, batchDeleteImages } from '../api/client'
import type { ImageInfo, PaginatedListParams } from '../api/client'
import { CheckSquare, Square, Trash2, X } from 'lucide-react'
import SearchBar from '../components/SearchBar'
import SortDropdown from '../components/SortDropdown'

export default function Gallery() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [search, setSearch] = useState('')
  const [sort, setSort] = useState<NonNullable<PaginatedListParams['sort']>>('created_at')
  const [order, setOrder] = useState<NonNullable<PaginatedListParams['order']>>('desc')
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [selectMode, setSelectMode] = useState(false)
  const [isDeleting, setIsDeleting] = useState(false)
  const [showConfirm, setShowConfirm] = useState(false)

  const { data, isLoading, isError, fetchNextPage, hasNextPage, isFetchingNextPage } =
    useInfiniteQuery({
      queryKey: ['images', { search, sort, order }],
      queryFn: ({ pageParam }) =>
        listImages({ page: pageParam, per_page: 20, sort, order, search }),
      initialPageParam: 1,
      getNextPageParam: (lastPage) =>
        lastPage.page < lastPage.total_pages ? lastPage.page + 1 : undefined,
      placeholderData: keepPreviousData,
    })

  const observerRef = useRef<IntersectionObserver>(undefined)
  const lastItemRef = useCallback(
    (node: HTMLButtonElement | null) => {
      if (isFetchingNextPage) return
      if (observerRef.current) observerRef.current.disconnect()
      observerRef.current = new IntersectionObserver(
        (entries) => { if (entries[0].isIntersecting && hasNextPage) fetchNextPage() },
        { rootMargin: '200px' },
      )
      if (node) observerRef.current.observe(node)
    },
    [isFetchingNextPage, hasNextPage, fetchNextPage],
  )
  useEffect(() => { return () => { observerRef.current?.disconnect() } }, [])

  const allImages: ImageInfo[] = data?.pages.flatMap((p) => p.items) ?? []
  const total = data?.pages[0]?.total ?? 0

  function toggleSelect(id: string) {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) {
        next.delete(id)
        if (next.size === 0) setSelectMode(false)
      } else {
        next.add(id)
        setSelectMode(true)
      }
      return next
    })
  }

  function toggleSelectAll() {
    if (selected.size === allImages.length) {
      setSelected(new Set())
      setSelectMode(false)
    } else {
      setSelected(new Set(allImages.map((img) => img.id)))
    }
  }

  function clearSelection() {
    setSelected(new Set())
    setSelectMode(false)
  }

  async function confirmDelete() {
    setShowConfirm(false)
    setIsDeleting(true)
    try {
      const ids = Array.from(selected)
      const result = await batchDeleteImages(ids)
      if (result.deleted > 0) {
        queryClient.invalidateQueries({ queryKey: ['images'] })
      }
      clearSelection()
    } catch {
      // ky hooks handle error toasts
    } finally {
      setIsDeleting(false)
    }
  }

  return (
    <div className="mx-auto max-w-5xl p-4">
      {/* Header */}
      <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h1 className="text-lg font-bold text-[var(--color-text-primary)]">
          Gallery{total > 0 && <span className="ml-2 text-sm font-normal text-[var(--color-text-muted)]">({total} images)</span>}
        </h1>
        <div className="flex items-center gap-3">
          <div className="w-48 sm:w-64"><SearchBar value={search} onChange={setSearch} /></div>
          <SortDropdown sort={sort} order={order}
            onSortChange={(s) => setSort(s as NonNullable<PaginatedListParams['sort']>)}
            onOrderChange={(o) => setOrder(o as NonNullable<PaginatedListParams['order']>)} />
        </div>
      </div>

      {/* Selection toolbar */}
      {selectMode && (
        <div className="mb-3 flex items-center justify-between rounded-lg border border-[var(--color-accent)] bg-[var(--color-accent-subtle)] px-3 py-2">
          <span className="text-sm text-[var(--color-text-primary)]">{selected.size} selected</span>
          <div className="flex items-center gap-2">
            <button onClick={toggleSelectAll} className="rounded px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-surface)]">
              {selected.size === allImages.length ? 'Deselect All' : 'Select All'}
            </button>
            <button onClick={() => setShowConfirm(true)} disabled={isDeleting}
              className="flex items-center gap-1 rounded px-2 py-1 text-xs text-red-400 hover:bg-red-950 hover:text-red-300 disabled:opacity-50">
              <Trash2 className="h-3 w-3" />Delete
            </button>
            <button onClick={clearSelection} className="rounded p-1 text-[var(--color-text-muted)] hover:text-[var(--color-text-primary)]">
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>
      )}

      {/* States */}
      {isLoading && <div className="flex min-h-[200px] items-center justify-center text-[var(--color-text-muted)]">Loading…</div>}
      {isError && <div className="flex min-h-[200px] items-center justify-center text-red-500">Failed to load images.</div>}
      {!isLoading && !isError && allImages.length === 0 && (
        <div className="flex min-h-[200px] flex-col items-center justify-center gap-2 text-[var(--color-text-muted)]">
          <p>No images found.</p>
          {search && <p className="text-sm">Try a different search term.</p>}
        </div>
      )}

      {/* Grid */}
      {allImages.length > 0 && (
        <>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
            {allImages.map((img, index) => {
              const isLast = index === allImages.length - 1
              const isSelected = selected.has(img.id)
              return (
                <div key={img.id} className="relative">
                  {selectMode && (
                    <button onClick={(e) => { e.stopPropagation(); toggleSelect(img.id) }}
                      className="absolute left-2 top-2 z-10 rounded bg-black/60 p-0.5 hover:bg-black/80">
                      {isSelected ? <CheckSquare className="h-4 w-4 text-[var(--color-accent)]" />
                        : <Square className="h-4 w-4 text-white/60" />}
                    </button>
                  )}
                  <button
                    ref={isLast ? lastItemRef : undefined}
                    onClick={() => { selectMode ? toggleSelect(img.id) : navigate(`/images/${img.id}`) }}
                    className={`aspect-square w-full overflow-hidden rounded-lg border bg-[var(--color-surface-glass)] backdrop-blur-sm ${
                      isSelected ? 'border-[var(--color-accent)] ring-2 ring-[var(--color-accent)]'
                        : 'border-[var(--color-border)]'}`}>
                    <img src={img.thumbnail_url ?? img.url} alt={img.original_name}
                      className="h-full w-full object-cover" loading="lazy" />
                    <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/80 to-transparent p-2">
                      <p className="truncate text-xs text-white">{img.original_name}</p>
                    </div>
                  </button>
                </div>
              )
            })}
          </div>
          {isFetchingNextPage && <div className="mt-4 flex items-center justify-center py-4 text-sm text-[var(--color-text-muted)]">Loading more…</div>}
          {!hasNextPage && allImages.length > 0 && <div className="mt-4 flex items-center justify-center py-4 text-sm text-[var(--color-text-muted)]">All {total} images loaded</div>}
        </>
      )}

      {/* Confirm dialog */}
      {showConfirm && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
          <div className="mx-4 w-full max-w-sm rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] p-6 shadow-xl">
            <h2 className="mb-2 text-lg font-semibold text-[var(--color-text-primary)]">
              Delete {selected.size} image{selected.size !== 1 ? 's' : ''}?
            </h2>
            <p className="mb-4 text-sm text-[var(--color-text-secondary)]">
              This cannot be undone. Images will be permanently deleted from storage.
            </p>
            <div className="flex justify-end gap-3">
              <button onClick={() => setShowConfirm(false)}
                className="rounded-lg px-4 py-2 text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-glass)]">Cancel</button>
              <button onClick={confirmDelete} disabled={isDeleting}
                className="rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50">
                {isDeleting ? 'Deleting…' : 'Delete'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
