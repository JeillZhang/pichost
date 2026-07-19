import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ChevronRight, Folder, FolderOpen, Plus } from 'lucide-react'
import { listCategories, type CategoryTreeNode } from '../api/client'

interface CategoryTreeProps {
  selectedId: string | null
  onSelect: (id: string | null) => void
  onAddCategory: (parentId: string | null) => void
  onEditCategory: (id: string, name: string) => void
  onDeleteCategory: (id: string) => void
}

function TreeNode({
  node,
  depth,
  selectedId,
  onSelect,
}: {
  node: CategoryTreeNode
  depth: number
  selectedId: string | null
  onSelect: (id: string | null) => void
  onAddCategory: (parentId: string | null) => void
  onEditCategory: (id: string, name: string) => void
  onDeleteCategory: (id: string) => void
}) {
  const [expanded, setExpanded] = useState(false)
  const hasChildren = node.children.length > 0
  const isSelected = selectedId === node.id

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
        onContextMenu={(e) => {
          e.preventDefault()
          // Context menu for rename/delete — handled in T7
        }}
      >
        {hasChildren && (
          <button
            onClick={(e) => {
              e.stopPropagation()
              setExpanded(!expanded)
            }}
            className="flex h-4 w-4 items-center justify-center"
          >
            <ChevronRight
              size={14}
              className={`transition-transform ${expanded ? 'rotate-90' : ''}`}
            />
          </button>
        )}
        {!hasChildren && <span className="w-4" />}
        {expanded ? <FolderOpen size={16} /> : <Folder size={16} />}
        <span className="flex-1 truncate">{node.name}</span>
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
              onAddCategory={() => {}}
              onEditCategory={() => {}}
              onDeleteCategory={() => {}}
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
  onAddCategory,
  onEditCategory,
  onDeleteCategory,
}: CategoryTreeProps) {
  const { data: categories, isLoading } = useQuery({
    queryKey: ['categories'],
    queryFn: listCategories,
  })

  return (
    <div className="flex flex-col">
      <div className="flex items-center justify-between px-2 py-2">
        <span className="text-xs font-medium uppercase tracking-wider text-[var(--color-text-muted)]">
          Categories
        </span>
        <button
          onClick={() => onAddCategory(null)}
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
        <div className="px-4 py-2 text-xs text-[var(--color-text-muted)]">Loading...</div>
      ) : categories && categories.length > 0 ? (
        <div className="mt-1">
          {categories.map((cat) => (
            <TreeNode
              key={cat.id}
              node={cat}
              depth={0}
              selectedId={selectedId}
              onSelect={onSelect}
              onAddCategory={onAddCategory}
              onEditCategory={onEditCategory}
              onDeleteCategory={onDeleteCategory}
            />
          ))}
        </div>
      ) : (
        <div className="px-4 py-4 text-center text-xs text-[var(--color-text-muted)]">
          No categories yet
          <br />
          <button
            onClick={() => onAddCategory(null)}
            className="mt-1 text-[var(--color-accent)] hover:underline"
          >
            Create one
          </button>
        </div>
      )}
    </div>
  )
}
