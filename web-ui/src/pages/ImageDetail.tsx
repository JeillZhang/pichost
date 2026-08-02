import { useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Pencil, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import {
  getImage,
  deleteImage,
  listCategories,
  moveImageToCategory,
  renameImage,
  type CategoryTreeNode,
} from '../api/client'
import LinkCard from '../components/LinkCard'
import GlassSelect from '../components/ui/GlassSelect'

function flattenCategories(
  nodes: CategoryTreeNode[] | undefined,
): { id: string; name: string; depth: number }[] {
  if (!nodes) return []
  const result: { id: string; name: string; depth: number }[] = []
  function walk(items: CategoryTreeNode[], depth: number) {
    for (const item of items) {
      result.push({ id: item.id, name: item.name, depth })
      if (item.children.length > 0) walk(item.children, depth + 1)
    }
  }
  walk(nodes, 0)
  return result
}

const STATUS_STYLES: Record<string, string> = {
  active: 'bg-[var(--color-success-subtle)] text-[var(--color-success)] border-[var(--color-success)] border-opacity-30',
  ready: 'bg-[var(--color-success-subtle)] text-[var(--color-success)] border-[var(--color-success)] border-opacity-30',
  processing:
    'bg-[var(--color-warning-subtle)] text-[var(--color-warning)] border-[var(--color-warning)] border-opacity-30',
  pending:
    'bg-[var(--color-accent-subtle)] text-[var(--color-accent)] border-[var(--color-accent)] border-opacity-30',
  failed:
    'bg-[var(--color-danger-subtle)] text-[var(--color-danger)] border-[var(--color-danger)] border-opacity-30',
}

const LINK_OPTIONS = [
  { value: 'url', label: 'URL' },
  { value: 'markdown', label: 'Markdown' },
  { value: 'html', label: 'HTML' },
  { value: 'bbcode', label: 'BBCode' },
] as const

type LinkFormat = (typeof LINK_OPTIONS)[number]['value']

export default function ImageDetail() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [deleting, setDeleting] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [isRenaming, setIsRenaming] = useState(false)
  const [renameValue, setRenameValue] = useState('')
  const [linkFormat, setLinkFormat] = useState<LinkFormat>('url')

  const { data: img, isLoading } = useQuery({
    queryKey: ['image', id],
    queryFn: () => getImage(id!),
    enabled: !!id,
  })

  const { data: categories } = useQuery({
    queryKey: ['categories'],
    queryFn: listCategories,
  })

  const moveMutation = useMutation({
    mutationFn: ({ imageId, categoryId }: { imageId: string; categoryId: string | null }) =>
      moveImageToCategory(imageId, categoryId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['image', id] })
      queryClient.invalidateQueries({ queryKey: ['images'] })
    },
  })

  const renameMutation = useMutation({
    mutationFn: ({ imageId, name }: { imageId: string; name: string }) =>
      renameImage(imageId, name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['image', id] })
      queryClient.invalidateQueries({ queryKey: ['images'] })
      setIsRenaming(false)
    },
    onError: (e: unknown) => {
      const msg = e instanceof Error ? e.message : 'Rename failed'
      toast.error(msg)
      setIsRenaming(false)
    },
  })

  async function handleDelete() {
    if (!confirmDelete) {
      setConfirmDelete(true)
      return
    }
    if (!id) return
    setDeleting(true)
    try {
      await deleteImage(id)
      toast.success('Image deleted')
      queryClient.invalidateQueries({ queryKey: ['images'] })
      navigate('/dashboard', { replace: true })
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Delete failed'
      toast.error(msg)
      setDeleting(false)
      setConfirmDelete(false)
    }
  }

  if (isLoading) {
    return (
      <div
        className="flex min-h-screen items-center justify-center"
        style={{ color: 'var(--color-text-muted)' }}
      >
        Loading…
      </div>
    )
  }

  if (!img) {
    return (
      <div
        className="flex min-h-screen items-center justify-center"
        style={{ color: 'var(--color-text-muted)' }}
      >
        Image not found.
      </div>
    )
  }

  const linkValues: Record<LinkFormat, string> = {
    url: img.url,
    markdown: img.markdown,
    html: img.html,
    bbcode: img.bbcode,
  }
  const selectedLinkLabel = LINK_OPTIONS.find((o) => o.value === linkFormat)!.label

  return (
    <div className="mx-auto max-w-4xl p-4">
      {/* Back button */}
      <button
        onClick={() => navigate(-1)}
        className="btn-ghost mb-4 px-3 py-1.5 text-sm"
      >
        <ArrowLeft className="h-4 w-4" />
        Back
      </button>

      {/* Image preview */}
      <div className="glass-elevated mb-4 overflow-hidden rounded-xl">
        <img
          src={img.url}
          alt={img.original_name}
          className="max-h-[60vh] w-full object-contain"
        />
      </div>

      {/* Info card — metadata + generated assets + links */}
      <div className="glass-elevated rounded-xl p-5">
        {/* ── Metadata ── */}
        <div className="flex items-center gap-2">
          <span style={{ color: 'var(--color-text-secondary)' }}>Name:</span>
          {isRenaming ? (
            <input
              autoFocus
              type="text"
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && renameValue.trim()) {
                  renameMutation.mutate({ imageId: id!, name: renameValue.trim() })
                } else if (e.key === 'Escape') {
                  setIsRenaming(false)
                }
              }}
              onBlur={() => setIsRenaming(false)}
              disabled={renameMutation.isPending}
              className="input-field flex-1 py-1"
            />
          ) : (
            <button
              onClick={() => {
                setRenameValue(img.original_name)
                setIsRenaming(true)
              }}
              className="group flex items-center gap-1 font-medium hover:opacity-80"
              style={{ color: 'var(--color-text-primary)' }}
            >
              <span>{img.original_name}</span>
              <Pencil className="h-3 w-3 opacity-0 transition-opacity group-hover:opacity-100" />
            </button>
          )}
          {renameMutation.isPending && (
            <span className="text-xs" style={{ color: 'var(--color-text-muted)' }}>
              Saving...
            </span>
          )}
        </div>

        <div className="mt-3 grid gap-x-6 gap-y-2 sm:grid-cols-2">
          <p className="flex items-center gap-2">
            <span style={{ color: 'var(--color-text-secondary)' }}>Status:</span>
            <span
              className={`badge ${STATUS_STYLES[img.status] || 'bg-[var(--color-surface)] text-[var(--color-text-secondary)]'}`}
            >
              {img.status}
            </span>
          </p>
          {img.width && img.height && (
            <p>
              <span style={{ color: 'var(--color-text-secondary)' }}>Dimensions:</span>{' '}
              <span style={{ color: 'var(--color-text-primary)' }}>
                {img.width} × {img.height}px
              </span>
            </p>
          )}
          <p>
            <span style={{ color: 'var(--color-text-secondary)' }}>Type:</span>{' '}
            <span style={{ color: 'var(--color-text-primary)' }}>{img.mime_type}</span>
          </p>
          <p>
            <span style={{ color: 'var(--color-text-secondary)' }}>Size:</span>{' '}
            <span style={{ color: 'var(--color-text-primary)' }}>
              {(img.file_size / 1024).toFixed(1)} KB
            </span>
          </p>
          <p className="sm:col-span-2">
            <span style={{ color: 'var(--color-text-secondary)' }}>Uploaded:</span>{' '}
            <span style={{ color: 'var(--color-text-primary)' }}>
              {new Date(img.created_at).toLocaleString()}
            </span>
          </p>
        </div>

        <div className="mt-4">
          <label
            className="mb-1.5 block text-xs font-semibold uppercase tracking-wider"
            style={{ color: 'var(--color-text-muted)' }}
          >
            Category
          </label>
          <GlassSelect
            value={img.category_id ?? ''}
            onChange={(v) => {
              // Empty string ("None") → null removes the category.
              moveMutation.mutate({ imageId: id!, categoryId: v || null })
            }}
            disabled={moveMutation.isPending}
            ariaLabel="Category"
            options={[
              { value: '', label: 'None' },
              ...flattenCategories(categories).map((c) => ({
                value: c.id,
                label: `${'  '.repeat(c.depth)}${c.name}`,
              })),
            ]}
          />
          {moveMutation.isPending && (
            <p className="mt-1 text-xs" style={{ color: 'var(--color-text-muted)' }}>
              Updating...
            </p>
          )}
        </div>

        {/* ── Generated Assets ── */}
        {(img.thumbnail_url || img.webp_url) && (
          <>
            <div className="divider my-4" />
            <p
              className="mb-2 text-xs font-semibold uppercase tracking-wider"
              style={{ color: 'var(--color-text-muted)' }}
            >
              Generated Assets
            </p>
            <div className="space-y-2">
              {img.thumbnail_url && (
                <LinkCard label="Thumbnail URL" value={img.thumbnail_url} />
              )}
              {img.webp_url && <LinkCard label="WebP URL" value={img.webp_url} />}
            </div>
          </>
        )}

        {/* ── Links ── */}
        <div className="divider my-4" />
        <label
          className="mb-1.5 block text-xs font-semibold uppercase tracking-wider"
          style={{ color: 'var(--color-text-muted)' }}
        >
          Links
        </label>
        <GlassSelect
          value={linkFormat}
          onChange={(v) => setLinkFormat(v as LinkFormat)}
          options={LINK_OPTIONS}
          ariaLabel="Link format"
          className="mb-2"
        />
        <LinkCard label={selectedLinkLabel} value={linkValues[linkFormat]} />
      </div>

      {/* Delete */}
      <div className="divider mt-6 pt-4">
        {confirmDelete ? (
          <div className="flex items-center gap-3">
            <button
              onClick={handleDelete}
              disabled={deleting}
              className="btn-accent"
              style={{ background: 'var(--color-danger)' }}
            >
              <Trash2 className="h-4 w-4" />
              {deleting ? 'Deleting…' : 'Confirm Delete'}
            </button>
            <button
              onClick={() => setConfirmDelete(false)}
              className="btn-ghost"
            >
              Cancel
            </button>
          </div>
        ) : (
          <button
            onClick={handleDelete}
            className="btn-ghost"
            style={{ color: 'var(--color-danger)', borderColor: 'var(--color-danger-border)' }}
          >
            <Trash2 className="h-4 w-4" />
            Delete Image
          </button>
        )}
      </div>
    </div>
  )
}
