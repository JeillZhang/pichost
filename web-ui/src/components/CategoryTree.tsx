import { useState, useEffect, useRef } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { ChevronRight, Folder, FolderOpen, Plus, Pencil, Trash2 } from 'lucide-react'
import {
  listCategories,
  createCategory,
  updateCategory,
  deleteCategory,
  type CategoryTreeNode,
} from '../api/client'

interface CategoryTreeProps {
  selectedId: string | null
  onSelect: (id: string | null) => void
}

interface ContextMenuState {
  x: number
  y: number
  nodeId: string
  nodeName: string
}

function TreeNode({
  node,
  depth,
  selectedId,
  onSelect,
  renameId,
  renameValue,
  setRenameId,
  setRenameValue,
  onRenameSubmit,
  onContextMenu,
}: {
  node: CategoryTreeNode
  depth: number
  selectedId: string | null
  onSelect: (id: string | null) => void
  renameId: string | null
  renameValue: string
  setRenameId: (id: string | null) => void
  setRenameValue: (v: string) => void
  onRenameSubmit: (id: string, name: string) => void
  onContextMenu: (e: React.MouseEvent, nodeId: string, nodeName: string) => void
}) {
  const [expanded, setExpanded] = useState(false)
  const hasChildren = node.children.length > 0
  const isSelected = selectedId === node.id
  const isRenaming = renameId === node.id

  return (
    <div>
      <div
        className={`group flex cursor-pointer items-center gap-1 rounded-md px-2 py-1 text-sm transition-colors ${
          isSelected
            ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)]'
            : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]'
        }`}
        style={{ paddingLeft: `${depth * 16 + 8}px` }}
        onClick={() => onSelect(isSelected ? null : node.id)}
        onContextMenu={(e) => onContextMenu(e, node.id, node.name)}
      >
        {hasChildren && (
          <button
            onClick={(e) => {
              e.stopPropagation()
              setExpanded(!expanded)
            }}
            className="flex h-4 w-4 shrink-0 items-center justify-center"
          >
            <ChevronRight
              size={14}
              className={`transition-transform ${expanded ? 'rotate-90' : ''}`}
            />
          </button>
        )}
        {!hasChildren && <span className="w-4 shrink-0" />}
        {expanded ? (
          <FolderOpen size={16} className="shrink-0" />
        ) : (
          <Folder size={16} className="shrink-0" />
        )}
        {isRenaming ? (
          <input
            value={renameValue}
            onChange={(e) => setRenameValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && renameValue.trim()) {
                onRenameSubmit(node.id, renameValue.trim())
              }
              if (e.key === 'Escape') setRenameId(null)
            }}
            onBlur={() => setRenameId(null)}
            className="flex-1 rounded border border-[var(--color-accent)] bg-[var(--color-surface)] px-1 py-0 text-sm text-[var(--color-text-primary)] outline-none"
            autoFocus
            onClick={(e) => e.stopPropagation()}
          />
        ) : (
          <span className="flex-1 truncate">{node.name}</span>
        )}
      </div>
      {expanded && hasChildren && (
        <div>
          {node.children.map((child) => (
            <TreeNode
              key={child.id}
              node={child}
              depth={depth + 1}
              selectedId={selectedId}
              onSelect={onSelect}
              renameId={renameId}
              renameValue={renameValue}
              setRenameId={setRenameId}
              setRenameValue={setRenameValue}
              onRenameSubmit={onRenameSubmit}
              onContextMenu={onContextMenu}
            />
          ))}
        </div>
      )}
    </div>
  )
}

export default function CategoryTree({
  selectedId,
  onSelect,
}: CategoryTreeProps) {
  const queryClient = useQueryClient()
  const contextMenuRef = useRef<HTMLDivElement>(null)

  // Create modal state
  const [showCreate, setShowCreate] = useState(false)
  const [createName, setCreateName] = useState('')
  const [createParentId, setCreateParentId] = useState<string | null>(null)

  // Inline rename state
  const [renameId, setRenameId] = useState<string | null>(null)
  const [renameValue, setRenameValue] = useState('')

  // Delete confirmation state
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null)

  // Context menu state
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null)

  const { data: categories, isLoading } = useQuery({
    queryKey: ['categories'],
    queryFn: listCategories,
  })

  const createMutation = useMutation({
    mutationFn: (data: { name: string; parent_id?: string | null }) =>
      createCategory(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['categories'] })
      setShowCreate(false)
      setCreateName('')
    },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: { name: string } }) =>
      updateCategory(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['categories'] })
      setRenameId(null)
    },
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteCategory(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['categories'] })
      setDeleteConfirmId(null)
    },
  })

  // Close context menu on outside click
  useEffect(() => {
    function handleClick() {
      setContextMenu(null)
    }
    if (contextMenu) {
      document.addEventListener('click', handleClick)
      return () => document.removeEventListener('click', handleClick)
    }
  }, [contextMenu])

  function handleContextMenu(
    e: React.MouseEvent,
    nodeId: string,
    nodeName: string,
  ) {
    e.preventDefault()
    e.stopPropagation()
    setContextMenu({ x: e.clientX, y: e.clientY, nodeId, nodeName })
  }

  function handleRenameSubmit(id: string, name: string) {
    updateMutation.mutate({ id, data: { name } })
  }

  function handleContextRename() {
    if (!contextMenu) return
    setRenameId(contextMenu.nodeId)
    setRenameValue(contextMenu.nodeName)
    setContextMenu(null)
  }

  function handleContextDelete() {
    if (!contextMenu) return
    setDeleteConfirmId(contextMenu.nodeId)
    setContextMenu(null)
  }

  return (
    <div className="flex flex-col">
      <div className="flex items-center justify-between px-2 py-2">
        <span className="text-xs font-medium uppercase tracking-wider text-[var(--color-text-muted)]">
          Categories
        </span>
        <button
          onClick={() => {
            setCreateParentId(null)
            setShowCreate(true)
            setCreateName('')
          }}
          className="rounded p-1 text-[var(--color-text-muted)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]"
          title="New category"
        >
          <Plus size={16} />
        </button>
      </div>

      {/* "All Images" option */}
      <div
        className={`cursor-pointer rounded-md px-2 py-1.5 text-sm transition-colors ${
          selectedId === null
            ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)] font-medium'
            : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-surface)]'
        }`}
        style={{ paddingLeft: '8px' }}
        onClick={() => onSelect(null)}
      >
        All Images
      </div>

      {isLoading ? (
        <div className="px-4 py-2 text-xs text-[var(--color-text-muted)]">
          Loading...
        </div>
      ) : categories && categories.length > 0 ? (
        <div className="mt-1">
          {categories.map((cat) => (
            <TreeNode
              key={cat.id}
              node={cat}
              depth={0}
              selectedId={selectedId}
              onSelect={onSelect}
              renameId={renameId}
              renameValue={renameValue}
              setRenameId={setRenameId}
              setRenameValue={setRenameValue}
              onRenameSubmit={handleRenameSubmit}
              onContextMenu={handleContextMenu}
            />
          ))}
        </div>
      ) : (
        <div className="px-4 py-4 text-center text-xs text-[var(--color-text-muted)]">
          No categories yet
          <br />
          <button
            onClick={() => {
              setCreateParentId(null)
              setShowCreate(true)
              setCreateName('')
            }}
            className="mt-1 text-[var(--color-accent)] hover:underline"
          >
            Create one
          </button>
        </div>
      )}

      {/* Context Menu */}
      {contextMenu && (
        <div
          ref={contextMenuRef}
          className="fixed z-50 min-w-[120px] rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-elevated)] py-1 shadow-lg"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            onClick={handleContextRename}
            className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm text-[var(--color-text-primary)] hover:bg-[var(--color-surface)]"
          >
            <Pencil size={14} />
            Rename
          </button>
          <button
            onClick={handleContextDelete}
            className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm text-red-500 hover:bg-[var(--color-surface)]"
          >
            <Trash2 size={14} />
            Delete
          </button>
        </div>
      )}

      {/* Create Modal */}
      {showCreate && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
          onClick={() => setShowCreate(false)}
        >
          <div
            className="w-80 rounded-xl border border-[var(--color-border)] bg-[var(--color-surface-elevated)] p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-3 text-sm font-medium text-[var(--color-text-primary)]">
              New Category
            </h3>
            <input
              type="text"
              value={createName}
              onChange={(e) => setCreateName(e.target.value)}
              placeholder="Category name"
              className="mb-3 w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-2 text-sm text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent)]"
              autoFocus
              onKeyDown={(e) => {
                if (e.key === 'Enter' && createName.trim()) {
                  createMutation.mutate({
                    name: createName.trim(),
                    parent_id: createParentId,
                  })
                }
                if (e.key === 'Escape') setShowCreate(false)
              }}
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setShowCreate(false)}
                className="rounded-lg px-3 py-1.5 text-sm text-[var(--color-text-muted)] hover:bg-[var(--color-surface)]"
              >
                Cancel
              </button>
              <button
                onClick={() => {
                  if (createName.trim()) {
                    createMutation.mutate({
                      name: createName.trim(),
                      parent_id: createParentId,
                    })
                  }
                }}
                disabled={!createName.trim() || createMutation.isPending}
                className="rounded-lg bg-[var(--color-accent)] px-3 py-1.5 text-sm text-white disabled:opacity-50"
              >
                {createMutation.isPending ? 'Creating...' : 'Create'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Delete Confirmation Dialog */}
      {deleteConfirmId && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
          onClick={() => setDeleteConfirmId(null)}
        >
          <div
            className="w-72 rounded-xl border border-[var(--color-border)] bg-[var(--color-surface-elevated)] p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-2 text-sm font-medium text-[var(--color-text-primary)]">
              Delete Category
            </h3>
            <p className="mb-4 text-xs text-[var(--color-text-muted)]">
              This will also delete all sub-categories. Images in these
              categories will be unassigned.
            </p>
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setDeleteConfirmId(null)}
                className="rounded-lg px-3 py-1.5 text-sm text-[var(--color-text-muted)] hover:bg-[var(--color-surface)]"
              >
                Cancel
              </button>
              <button
                onClick={() => deleteMutation.mutate(deleteConfirmId)}
                disabled={deleteMutation.isPending}
                className="rounded-lg bg-red-600 px-3 py-1.5 text-sm text-white disabled:opacity-50"
              >
                {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
