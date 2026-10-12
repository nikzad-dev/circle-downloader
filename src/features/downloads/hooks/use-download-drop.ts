import { useEffect, useState } from 'react'
import { toast } from '@/components/ui/toast'
import { openDownloadWindow } from '../components/download-window-api'
import { droppedLinks } from '../lib/dropped-links'
import { isEditableTarget } from '../lib/input-target'

export function useDownloadDrop() {
  const [dragging, setDragging] = useState(false)
  useEffect(() => {
    let depth = 0
    let internal = false
    const reset = () => {
      depth = 0
      setDragging(false)
    }
    const editable = (event: DragEvent) =>
      event.composedPath().some(isEditableTarget)
    const hasFiles = (event: DragEvent) =>
      Array.from(event.dataTransfer?.types ?? []).includes('Files')
    const hasText = (event: DragEvent) =>
      Array.from(event.dataTransfer?.types ?? []).some((type) =>
        ['text/uri-list', 'text/plain', 'text/x-moz-url'].includes(type),
      )
    const start = () => {
      internal = true
      reset()
    }
    const end = () => {
      internal = false
      reset()
    }
    const enter = (event: DragEvent) => {
      if (
        internal ||
        event.defaultPrevented ||
        editable(event) ||
        hasFiles(event) ||
        !hasText(event)
      )
        return
      event.preventDefault()
      depth += 1
      setDragging(true)
    }
    const over = (event: DragEvent) => {
      if (internal || event.defaultPrevented || editable(event)) {
        reset()
        return
      }
      if (hasFiles(event)) {
        event.preventDefault()
        if (event.dataTransfer) event.dataTransfer.dropEffect = 'none'
        reset()
        return
      }
      if (!hasText(event)) return
      event.preventDefault()
      if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy'
      setDragging(true)
    }
    const leave = () => {
      depth = Math.max(0, depth - 1)
      if (depth === 0) reset()
    }
    const drop = (event: DragEvent) => {
      reset()
      if (
        internal ||
        event.defaultPrevented ||
        editable(event) ||
        !event.dataTransfer
      )
        return
      // Never let an external drop navigate the webview away from the app.
      event.preventDefault()
      const links = hasFiles(event) ? [] : droppedLinks(event.dataTransfer)
      if (!links.length) {
        toast.add({
          title: 'Drop a download link',
          description:
            'Drag an HTTP or HTTPS link, rather than a local file or other text.',
          type: 'error',
        })
        return
      }
      if (links.length > 200 || links.some((link) => link.length > 8192)) {
        toast.add({
          title: 'Too many or oversized links',
          description:
            'Drop up to 200 links, each no longer than 8,192 characters.',
          type: 'error',
        })
        return
      }
      void openDownloadWindow(
        links.length === 1 ? links[0] : '',
        links.length > 1,
        links.length > 1 ? links : undefined,
      )
    }
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') end()
    }
    window.addEventListener('dragstart', start)
    window.addEventListener('dragend', end)
    window.addEventListener('dragenter', enter)
    window.addEventListener('dragover', over)
    window.addEventListener('dragleave', leave)
    window.addEventListener('drop', drop)
    window.addEventListener('blur', end)
    window.addEventListener('keydown', escape)
    return () => {
      window.removeEventListener('dragstart', start)
      window.removeEventListener('dragend', end)
      window.removeEventListener('dragenter', enter)
      window.removeEventListener('dragover', over)
      window.removeEventListener('dragleave', leave)
      window.removeEventListener('drop', drop)
      window.removeEventListener('blur', end)
      window.removeEventListener('keydown', escape)
    }
  }, [])
  return dragging
}
