import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useProcessingActorRef } from '@/lib/machines'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'

export type UseNavigatorFileDropOptions = {
  panelRef: React.RefObject<HTMLElement | null>
  viewportRef: React.RefObject<HTMLElement | null>
  rowHeight: number
  totalPages: number
}

function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in window
}

export function useNavigatorFileDrop(options: UseNavigatorFileDropOptions) {
  const { t } = useTranslation()
  const [isDragging, setIsDragging] = useState(false)
  const [insertIndex, setInsertIndex] = useState<number | null>(null)
  const actorRef = useProcessingActorRef()
  const showError = useEditorUiStore((state) => state.showError)

  const totalPagesRef = useRef(options.totalPages)
  totalPagesRef.current = options.totalPages

  const computeInsertIndex = useCallback(
    (clientX: number, clientY: number): number | null => {
      const panel = options.panelRef.current
      const viewport = options.viewportRef.current
      if (!panel || !viewport) return null

      const panelRect = panel.getBoundingClientRect()
      if (
        clientX < panelRect.left ||
        clientX > panelRect.right ||
        clientY < panelRect.top ||
        clientY > panelRect.bottom
      ) {
        return null
      }

      const viewportRect = viewport.getBoundingClientRect()
      const relativeY = clientY - viewportRect.top + viewport.scrollTop
      const idx = Math.round(relativeY / options.rowHeight)
      return Math.max(0, Math.min(idx, totalPagesRef.current))
    },
    [options.panelRef, options.viewportRef, options.rowHeight],
  )

  // Autoscroll when cursor near navigator edges to reach drop position
  const EDGE_ZONE = 40 // px from top/bottom to trigger scroll
  const SCROLL_SPEED = 8 // px per frame
  const scrollRafRef = useRef<number | null>(null)
  const lastCursorYRef = useRef<number | null>(null)

  const startAutoScroll = useCallback(() => {
    const tick = () => {
      const viewport = options.viewportRef.current
      const cursorY = lastCursorYRef.current
      if (!viewport || cursorY === null) return

      const rect = viewport.getBoundingClientRect()
      if (cursorY < rect.top + EDGE_ZONE) {
        viewport.scrollTop -= SCROLL_SPEED
      } else if (cursorY > rect.bottom - EDGE_ZONE) {
        viewport.scrollTop += SCROLL_SPEED
      }
      scrollRafRef.current = requestAnimationFrame(tick)
    }
    if (scrollRafRef.current === null) {
      scrollRafRef.current = requestAnimationFrame(tick)
    }
  }, [options.viewportRef])

  const stopAutoScroll = useCallback(() => {
    if (scrollRafRef.current !== null) {
      cancelAnimationFrame(scrollRafRef.current)
      scrollRafRef.current = null
    }
    lastCursorYRef.current = null
  }, [])

  useEffect(() => {
    if (!isTauri()) return

    let unlisten: (() => void) | null = null
    let cancelled = false

    const setup = async () => {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview')
      if (cancelled) return

      unlisten = await getCurrentWebview().onDragDropEvent((event) => {
        const payload = event.payload

        if (payload.type === 'enter') {
          const idx = computeInsertIndex(payload.position.x, payload.position.y)
          if (idx !== null) {
            setIsDragging(true)
            setInsertIndex(idx)
            lastCursorYRef.current = payload.position.y
            startAutoScroll()
          }
        } else if (payload.type === 'over') {
          const idx = computeInsertIndex(payload.position.x, payload.position.y)
          lastCursorYRef.current = payload.position.y
          if (idx !== null) {
            setIsDragging(true)
            setInsertIndex(idx)
          } else {
            setIsDragging(false)
            setInsertIndex(null)
            stopAutoScroll()
          }
        } else if (payload.type === 'leave') {
          setIsDragging(false)
          setInsertIndex(null)
          stopAutoScroll()
        } else if (payload.type === 'drop') {
          const idx = computeInsertIndex(payload.position.x, payload.position.y)
          setIsDragging(false)
          setInsertIndex(null)
          stopAutoScroll()

          if (payload.paths.length === 0) {
            showError(t('dropZone.noFilePaths'))
            return
          }

          if (idx === null) {
            // Dropped outside panel bounds ignore
            return
          }

          actorRef.send({
            type: 'START_DROP_IMPORT_PATHS',
            paths: payload.paths,
            insertAt: idx,
          })
        }
      })
    }

    setup()
    return () => {
      cancelled = true
      unlisten?.()
      stopAutoScroll()
    }
  }, [actorRef, computeInsertIndex, startAutoScroll, stopAutoScroll, t])

  return { isDragging, insertIndex }
}
