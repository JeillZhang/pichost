import { useEffect } from 'react'

export function useClipboardPaste(onPaste: (files: File[]) => void) {
  useEffect(() => {
    const handler = (e: ClipboardEvent) => {
      const items = e.clipboardData?.items
      if (!items) return

      for (let i = 0; i < items.length; i++) {
        const item = items[i]
        if (item.kind === 'file' && item.type.startsWith('image/')) {
          const file = item.getAsFile()
          if (file) {
            onPaste([file])
            break
          }
        }
      }
    }

    document.addEventListener('paste', handler)
    return () => document.removeEventListener('paste', handler)
  }, [onPaste])
}
