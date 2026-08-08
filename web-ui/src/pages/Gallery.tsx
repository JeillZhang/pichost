import { useRef, useCallback, useEffect, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { useInfiniteQuery, keepPreviousData, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { listImages, batchDeleteImages, listStorageConfigs } from '../api/client'
import type { ImageInfo, PaginatedListParams } from '../api/client'
import { CheckSquare, Square, Trash2, X, Code2, Server, HardDrive, Folder } from 'lucide-react'
import SearchBar from '../components/SearchBar'
import SortDropdown from '../components/SortDropdown'
import CategoryTree from '../components/CategoryTree'
import GlassSelect from '../components/ui/GlassSelect'
import Sheet from '../components/ui/Sheet'
import ConfirmDialog from '../components/ui/ConfirmDialog'
import { toast } from 'sonner'

const STORAGE_CONFIG_KEY = 'backend'

function getProviderIcon(provider: string) {
  switch (provider) {
    case 'github':
      return <Code2 className="h-3 w-3" />
    case 'gitcode':
      return <Server className="h-3 w-3" />
    default:
      return <HardDrive className="h-3 w-3" />
  }
}

export default function Gallery() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [searchParams, setSearchParams] = useSearchParams()

  const [search, setSearch] = useState('')
  const [sort, setSort] = useState<NonNullable<PaginatedListParams['sort']>>('created_at')
  const [order, setOrder] = useState<NonNullable<PaginatedListParams['order']>>('desc')
  const [storageConfigFilter, setStorageConfigFilter] = useState(
    () => searchParams.get(STORAGE_CONFIG_KEY) ?? '',
  )
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [categoryFilter, setCategoryFilter] = useState<string | null>(null)
  const [categorySheetOpen, setCategorySheetOpen] = useState(false)
  const [selectMode, setSelectMode] = useState(false)
  const [isDeleting, setIsDeleting] = useState(false)
  const [showConfirm, setShowConfirm] = useState(false)

  const { data: storageConfigs } = useQuery({
    queryKey: ['storage-configs'],
    queryFn: () => listStorageConfigs(),
    staleTime: 5 * 60 * 1000,
  })

  const { data, isLoading, isError, fetchNextPage, hasNextPage, isFetchingNextPage } =
    useInfiniteQuery({
      queryKey: ['images', { search, sort, order, storageConfigFilter, categoryFilter }],
      queryFn: ({ pageParam }) =>
        listImages({
          page: pageParam,
          per_page: 20,
          sort,
          order,
          search,
          storage_config_id: storageConfigFilter || undefined,
          category_id: categoryFilter || undefined,
        }),
      initialPageParam: 1,
      getNextPageParam: (lastPage) =>
        lastPage.page < lastPage.total_pages ? lastPage.page + 1 : undefined,
      placeholderData: keepPreviousData,
    })

  useEffect(() => {
    const params = new URLSearchParams(searchParams)
    if (storageConfigFilter) {
      params.set(STORAGE_CONFIG_KEY, storageConfigFilter)
    } else {
      params.delete(STORAGE_CONFIG_KEY)
    }
    setSearchParams(params, { replace: true })
  }, [storageConfigFilter, searchParams, setSearchParams])

  useEffect(() => {
    const catFromUrl = searchParams.get('category_id')
    if (catFromUrl) setCategoryFilter(catFromUrl)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    const params = new URLSearchParams(searchParams)
    if (categoryFilter) {
      params.set('category_id', categoryFilter)
    } else {
      params.delete('category_id')
    }
    setSearchParams(params, { replace: true })
  }, [categoryFilter])

  const observerRef = useRef<IntersectionObserver>(undefined)
  const lastItemRef = useCallback(
    (node: HTMLButtonElement | null) => {
      if (isFetchingNextPage) return
      if (observerRef.current) observerRef.current.disconnect()
      observerRef.current = new IntersectionObserver(
        (entries) => {
          if (entries[0].isIntersecting && hasNextPage) fetchNextPage()
        },
        { rootMargin: '200px' },
      )
      if (node) observerRef.current.observe(node)
    },
    [isFetchingNextPage, hasNextPage, fetchNextPage],
  )
  useEffect(() => {
    return () => {
      observerRef.current?.disconnect()
    }
  }, [])

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

  const handleBatchMove = () => {
    toast.info(t('gallery.batchMovePlaceholder', { count: selected.size }))
  }

  return (
    <div className="mx-auto flex max-w-7xl gap-4 p-4">
      {/* Sidebar */}
      <aside className="hidden w-56 shrink-0 md:block">
        <div className="glass sticky top-16 rounded-lg p-2">
          <CategoryTree selectedId={categoryFilter} onSelect={setCategoryFilter} />
        </div>
      </aside>

      {/* Main content */}
      <div className="min-w-0 flex-1">
        {/* Header */}
        <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <h1
            className="text-lg font-bold"
            style={{ color: 'var(--color-text-primary)', fontFamily: "'Outfit', system-ui, sans-serif" }}
          >
            {t('gallery.title')}
            {total > 0 && (
              <span
                className="ml-2 text-sm font-normal"
                style={{ color: 'var(--color-text-muted)' }}
              >
                ({t('gallery.imagesCount', { count: total })})
              </span>
            )}
          </h1>
          <div className="flex items-center gap-2">
            <button
              onClick={() => setCategorySheetOpen(true)}
              className="flex items-center gap-1.5 rounded-lg border border-[var(--glass-border-base)] bg-[var(--glass-tint-base)]/65 px-3 py-1.5 text-sm md:hidden"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              <Folder className="h-4 w-4" />
              {t('gallery.allCategories')}
            </button>
            <div className="w-48 sm:w-64">
              <SearchBar value={search} onChange={setSearch} />
            </div>

            {storageConfigs && storageConfigs.length > 0 && (
              <div className="w-44">
                <GlassSelect
                  value={storageConfigFilter}
                  onChange={setStorageConfigFilter}
                  options={[
                    { value: '', label: t('gallery.allBackends') },
                    ...storageConfigs.map((c) => ({ value: c.id, label: c.name })),
                  ]}
                />
              </div>
            )}

            <SortDropdown
              sort={sort}
              order={order}
              onSortChange={(s) => setSort(s as NonNullable<PaginatedListParams['sort']>)}
              onOrderChange={(o) => setOrder(o as NonNullable<PaginatedListParams['order']>)}
            />
          </div>
        </div>

        {/* Selection toolbar */}
        {selectMode && (
          <div
            className="glass mb-3 flex items-center justify-between rounded-lg px-3 py-2"
            style={{
              borderColor: 'var(--color-accent-strong)',
              backgroundColor: 'var(--color-accent-subtle)',
            }}
          >
            <span className="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
              {t('gallery.selected', { count: selected.size })}
            </span>
            <div className="flex items-center gap-2">
              <button
                onClick={toggleSelectAll}
                className="rounded-lg px-2 py-1 text-xs transition-colors duration-150 hover:bg-[var(--color-surface-hover)]"
                style={{ color: 'var(--color-text-secondary)' }}
              >
                {selected.size === allImages.length ? t('gallery.deselectAll') : t('gallery.selectAll')}
              </button>
              <button
                onClick={() => setShowConfirm(true)}
                disabled={isDeleting}
                className="flex items-center gap-1 rounded-lg px-2 py-1 text-xs font-medium transition-colors duration-150 disabled:opacity-50"
                style={{
                  color: 'var(--color-danger)',
                  backgroundColor: 'var(--color-danger-subtle)',
                }}
              >
                <Trash2 className="h-3 w-3" />
                {t('gallery.delete')}
              </button>
              <button
                onClick={handleBatchMove}
                className="btn-ghost px-3 py-1.5 text-xs"
              >
                {t('gallery.moveToCategory')}
              </button>
              <button
                onClick={clearSelection}
                className="rounded-lg p-1 transition-colors duration-150 hover:text-[var(--color-text-primary)]"
                style={{ color: 'var(--color-text-muted)' }}
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          </div>
        )}

        {/* States */}
        {isLoading && (
          <div
            className="flex min-h-[200px] items-center justify-center"
            style={{ color: 'var(--color-text-muted)' }}
          >
            {t('gallery.loading')}
          </div>
        )}
        {isError && (
          <div className="flex min-h-[200px] items-center justify-center" style={{ color: 'var(--color-danger)' }}>
            {t('gallery.failedToLoad')}
          </div>
        )}
        {!isLoading && !isError && allImages.length === 0 && (
          <div
            className="flex min-h-[200px] flex-col items-center justify-center gap-2"
            style={{ color: 'var(--color-text-muted)' }}
          >
            <p>{t('gallery.noImagesFound')}</p>
            {search && <p className="text-sm">{t('gallery.tryDifferentSearch')}</p>}
          </div>
        )}

        {/* Grid */}
        {allImages.length > 0 && (
          <>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
              {allImages.map((img, index) => {
                const isLast = index === allImages.length - 1
                const isSelected = selected.has(img.id)
                return (
                  <div key={img.id} className="group relative">
                    {/* Always-visible selection toggle — this is the entry
                        point into select mode (tiles themselves navigate). */}
                    <button
                      onClick={(e) => {
                        e.stopPropagation()
                        toggleSelect(img.id)
                      }}
                      aria-label={t('gallery.selectAria', { name: img.original_name })}
                      className={`absolute left-2 top-2 z-10 rounded-lg p-1 backdrop-blur-sm transition-all duration-150 ${
                        selectMode
                          ? 'bg-black/50 hover:bg-black/70'
                          : 'bg-black/30 opacity-100 hover:bg-black/50 md:opacity-60 md:group-hover:opacity-100'
                      }`}
                    >
                      {isSelected ? (
                        <CheckSquare className="h-4 w-4" style={{ color: 'var(--color-accent)' }} />
                      ) : (
                        <Square className="h-4 w-4 text-white/60" />
                      )}
                    </button>
                    {/* Provider badge */}
                    {!selectMode && img.storage_config && (
                      <span className="absolute right-2 top-2 z-10 flex items-center gap-1 rounded-md bg-black/50 px-1.5 py-0.5 text-[10px] text-white/80 backdrop-blur-sm">
                        {getProviderIcon(img.storage_config.provider)}
                        {img.storage_config.name}
                      </span>
                    )}
                    <button
                      ref={isLast ? lastItemRef : undefined}
                      onClick={() => {
                        selectMode ? toggleSelect(img.id) : navigate(`/images/${img.id}`)
                      }}
                      className={`glass aspect-square w-full overflow-hidden rounded-lg p-0 transition-all duration-200 ${
                        isSelected
                          ? 'ring-2 ring-[var(--color-accent)]'
                          : 'group-hover:border-[var(--glass-border-strong)]'
                      }`}
                    >
                      <img
                        src={img.thumbnail_url ?? img.url}
                        alt={img.original_name}
                        className="h-full w-full object-cover"
                        loading="lazy"
                      />
                      <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent p-2">
                        <p className="truncate text-xs font-medium text-white">
                          {img.original_name}
                        </p>
                      </div>
                    </button>
                  </div>
                )
              })}
            </div>
            {isFetchingNextPage && (
              <div
                className="mt-4 flex items-center justify-center py-4 text-sm"
                style={{ color: 'var(--color-text-muted)' }}
              >
                {t('gallery.loadingMore')}
              </div>
            )}
            {!hasNextPage && allImages.length > 0 && (
              <div
                className="mt-4 flex items-center justify-center py-4 text-sm"
                style={{ color: 'var(--color-text-muted)' }}
              >
                {t('gallery.allLoaded', { count: total })}
              </div>
            )}
          </>
        )}

        {/* Mobile category drawer */}
        <Sheet
          open={categorySheetOpen}
          onClose={() => setCategorySheetOpen(false)}
          title={t('categoryTree.categories')}
        >
          <CategoryTree
            selectedId={categoryFilter}
            onSelect={(id) => {
              setCategoryFilter(id)
              setCategorySheetOpen(false)
            }}
          />
        </Sheet>

        {/* Confirm dialog */}
        <ConfirmDialog
          open={showConfirm}
          onClose={() => setShowConfirm(false)}
          onConfirm={confirmDelete}
          title={t('gallery.deleteConfirm', { count: selected.size })}
          message="This cannot be undone. Images will be permanently deleted from storage."
          confirmLabel={t('gallery.delete')}
          cancelLabel={t('gallery.cancel')}
          danger
          pending={isDeleting}
        />
      </div>
    </div>
  )
}
