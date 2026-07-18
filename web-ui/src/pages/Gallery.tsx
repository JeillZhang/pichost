import { useRef, useCallback, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useInfiniteQuery } from '@tanstack/react-query'
import { listImages } from '../api/client'
import type { ImageInfo, PaginatedListParams } from '../api/client'
import SearchBar from '../components/SearchBar'
import SortDropdown from '../components/SortDropdown'

export default function Gallery() {
  const navigate = useNavigate()
  const [search, setSearch] = useState('')
  const [sort, setSort] = useState<NonNullable<PaginatedListParams['sort']>>('created_at')
  const [order, setOrder] = useState<NonNullable<PaginatedListParams['order']>>('desc')

  const {
    data,
    isLoading,
    isError,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
  } = useInfiniteQuery({
    queryKey: ['images', { search, sort, order }],
    queryFn: ({ pageParam }) =>
      listImages({ page: pageParam, per_page: 20, sort, order, search }),
    initialPageParam: 1,
    getNextPageParam: (lastPage) => {
      if (lastPage.page < lastPage.total_pages) {
        return lastPage.page + 1
      }
      return undefined
    },
  })

  // Infinite scroll sentinel
  const observerRef = useRef<IntersectionObserver>(undefined)

  const lastItemRef = useCallback(
    (node: HTMLButtonElement | null) => {
      if (isFetchingNextPage) return
      if (observerRef.current) observerRef.current.disconnect()
      observerRef.current = new IntersectionObserver(
        (entries) => {
          if (entries[0].isIntersecting && hasNextPage) {
            fetchNextPage()
          }
        },
        { rootMargin: '200px' },
      )
      if (node) observerRef.current.observe(node)
    },
    [isFetchingNextPage, hasNextPage, fetchNextPage],
  )

  const allImages: ImageInfo[] = data?.pages.flatMap((p) => p.items) ?? []
  const total = data?.pages[0]?.total ?? 0

  return (
    <div className="mx-auto max-w-5xl p-4">
      {/* Header row */}
      <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h1 className="text-lg font-bold text-[var(--color-text-primary)]">
          Gallery
          {total > 0 && (
            <span className="ml-2 text-sm font-normal text-[var(--color-text-muted)]">
              ({total} images)
            </span>
          )}
        </h1>
        <div className="flex items-center gap-3">
          <div className="w-48 sm:w-64">
            <SearchBar value={search} onChange={setSearch} />
          </div>
          <SortDropdown
            sort={sort}
            order={order}
            onSortChange={(s) => setSort(s as NonNullable<PaginatedListParams['sort']>)}
            onOrderChange={(o) => setOrder(o as NonNullable<PaginatedListParams['order']>)}
          />
        </div>
      </div>

      {/* Loading state */}
      {isLoading && (
        <div className="flex min-h-[200px] items-center justify-center text-[var(--color-text-muted)]">
          Loading…
        </div>
      )}

      {/* Error state */}
      {isError && (
        <div className="flex min-h-[200px] items-center justify-center text-red-500">
          Failed to load images. Please try again.
        </div>
      )}

      {/* Empty state */}
      {!isLoading && !isError && allImages.length === 0 && (
        <div className="flex min-h-[200px] flex-col items-center justify-center gap-2 text-[var(--color-text-muted)]">
          <p>No images found.</p>
          {search && <p className="text-sm">Try a different search term.</p>}
        </div>
      )}

      {/* Image grid */}
      {allImages.length > 0 && (
        <>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
            {allImages.map((img, index) => {
              const isLast = index === allImages.length - 1
              return (
                <button
                  key={img.id}
                  ref={isLast ? lastItemRef : undefined}
                  onClick={() => navigate(`/images/${img.id}`)}
                  className="group relative aspect-square overflow-hidden rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] backdrop-blur-sm"
                >
                  <img
                    src={img.thumbnail_url ?? img.url}
                    alt={img.original_name}
                    className="h-full w-full object-cover transition-transform group-hover:scale-105"
                    loading="lazy"
                  />
                  <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/80 to-transparent p-2">
                    <p className="truncate text-xs text-white">
                      {img.original_name}
                    </p>
                  </div>
                </button>
              )
            })}
          </div>

          {/* Loading more indicator */}
          {isFetchingNextPage && (
            <div className="mt-4 flex items-center justify-center py-4 text-sm text-[var(--color-text-muted)]">
              Loading more…
            </div>
          )}

          {/* End of results */}
          {!hasNextPage && allImages.length > 0 && (
            <div className="mt-4 flex items-center justify-center py-4 text-sm text-[var(--color-text-muted)]">
              All {total} images loaded
            </div>
          )}
        </>
      )}
    </div>
  )
}
