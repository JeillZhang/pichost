import { useState, useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  ChevronRight,
  Folder,
  FolderOpen,
  Plus,
  Pencil,
  Trash2,
  MoreHorizontal,
} from 'lucide-react'
import Modal from './ui/Modal'
import ConfirmDialog from './ui/ConfirmDialog'
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
  anchor?: 'cursor' | 'button'
  buttonRect?: DOMRect
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
  onMenuButtonClick,
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
  onMenuButtonClick: (e: React.MouseEvent, nodeId: string, nodeName: string) => void
}) {
  const { t } = useTranslation()
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
        <button
          onClick={(e) => {
            e.stopPropagation()
            onMenuButtonClick(e, node.id, node.name)
          }}
          aria-label={t('categoryTree.moreActions')}
          className="rounded p-1 opacity-60 transition-opacity hover:opacity-100 md:opacity-0 md:group-hover:opacity-100"
          style={{ color: 'var(--color-text-muted)' }}
        >
          <MoreHorizontal size={14} />
        </button>
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
              onMenuButtonClick={onMenuButtonClick}
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
  const { t } = useTranslation()
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
    setContextMenu({ x: e.clientX, y: e.clientY, nodeId, nodeName, anchor: 'cursor' })
  }

  function handleMenuButtonClick(
    e: React.MouseEvent,
    nodeId: string,
    nodeName: string,
  ) {
    e.stopPropagation()
    const rect = e.currentTarget.getBoundingClientRect()
    setContextMenu({
      x: rect.left,
      y: rect.bottom + 4,
      nodeId,
      nodeName,
      anchor: 'button',
      buttonRect: rect,
    })
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

  const menuX = contextMenu
    ? Math.min(contextMenu.x, window.innerWidth - 160)
    : 0
  const menuY = contextMenu
    ? Math.min(contextMenu.y, window.innerHeight - 100)
    : 0

  return (
    <div className="flex flex-col">
      <div className="flex items-center justify-between px-2 py-2">
        <span className="text-xs font-medium uppercase tracking-wider text-[var(--color-text-muted)]">
          {t('categoryTree.categories')}
        </span>
        <button
          onClick={() => {
            setCreateParentId(null)
            setShowCreate(true)
            setCreateName('')
          }}
          className="rounded p-1 text-[var(--color-text-muted)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]"
          title={t('categoryTree.newCategoryTitle')}
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
        {t('categoryTree.allImages')}
      </div>

      {isLoading ? (
        <div className="px-4 py-2 text-xs text-[var(--color-text-muted)]">
          {t('categoryTree.loading')}
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
              onMenuButtonClick={handleMenuButtonClick}
            />
          ))}
        </div>
      ) : (
        <div className="px-4 py-4 text-center text-xs text-[var(--color-text-muted)]">
          {t('categoryTree.noCategories')}
          <br />
          <button
            onClick={() => {
              setCreateParentId(null)
              setShowCreate(true)
              setCreateName('')
            }}
            className="mt-1 text-[var(--color-accent)] hover:underline"
          >
            {t('categoryTree.createOne')}
          </button>
        </div>
      )}

      {/* Context Menu */}
      {contextMenu &&
        createPortal(
          <div
            ref={contextMenuRef}
            className="glass-elevated fixed z-50 min-w-[130px] rounded-lg py-1"
            style={{ left: menuX, top: menuY }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              onClick={handleContextRename}
              className="flex w-full items-center gap-2 px-3.5 py-2 text-left text-sm transition-colors duration-100 hover:bg-[var(--color-surface-hover)]"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              <Pencil size={14} />
              {t('categoryTree.rename')}
            </button>
            <button
              onClick={handleContextDelete}
              className="flex w-full items-center gap-2 px-3.5 py-2 text-left text-sm transition-colors duration-100 hover:bg-[var(--color-danger-subtle)]"
              style={{ color: 'var(--color-danger)' }}
            >
              <Trash2 size={14} />
              {t('categoryTree.delete')}
            </button>
          </div>,
          document.body,
        )}

      {/* Create Modal */}
      <Modal
        open={showCreate}
        onClose={() => setShowCreate(false)}
        title={t('categoryTree.newCategory')}
        size="sm"
      >
        <input
          type="text"
          value={createName}
          onChange={(e) => setCreateName(e.target.value)}
          placeholder={t('categoryTree.categoryName')}
          className="input-field mb-3"
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
        {createParentId && (
          <p className="mb-3 text-xs" style={{ color: 'var(--color-text-muted)' }}>
            {t('categoryTree.subCategoryNote')}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <button onClick={() => setShowCreate(false)} className="btn-ghost text-xs">
            {t('categoryTree.cancel')}
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
            className="btn-accent text-xs"
          >
            {createMutation.isPending ? t('categoryTree.creating') : t('categoryTree.create')}
          </button>
        </div>
      </Modal>

      {/* Delete Confirmation Dialog */}
      <ConfirmDialog
        open={!!deleteConfirmId}
        onClose={() => setDeleteConfirmId(null)}
        onConfirm={() => deleteMutation.mutate(deleteConfirmId!)}
        title={t('categoryTree.deleteCategory')}
        message={t('categoryTree.deleteConfirm')}
        confirmLabel={t('categoryTree.delete')}
        danger
        pending={deleteMutation.isPending}
      />
    </div>
  )
}
