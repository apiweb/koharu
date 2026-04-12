'use client'

import { isTauri } from '@/lib/backend'
import { IMAGE_EXTENSIONS } from '@/lib/filePicker'
import i18n from '@/lib/i18n'
import { useProcessingActorRef } from '@/lib/machines'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'
import { useCallback, useEffect, useRef, useState } from 'react'

type DragDropPayload =
  | { type: 'drop'; paths: string[]; position: { x: number; y: number } }
  | { type: 'over'; position: { x: number; y: number } }
  | { type: 'enter'; paths: string[]; position: { x: number; y: number } }
  | { type: 'leave' }

function isAllowedImageFile(file: File): boolean {
  if (file.type.startsWith('image/')) return true
  return IMAGE_EXTENSIONS.some((ext) => file.name.toLowerCase().endsWith(ext))
}

function showError(key: string) {
  useEditorUiStore.getState().showError(i18n.t(key))
}

function extractImageUrls(uriList: string): string[] {
  return uriList
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith('#'))
    .filter((url) => url.startsWith('http://') || url.startsWith('https://'))
}

// ---------------------------------------------------------------------------
// useNavigatorFileDrop
// ---------------------------------------------------------------------------

export type UseNavigatorFileDropOptions = {
  panelRef: React.RefObject<HTMLElement | null>
  viewportRef: React.RefObject<HTMLElement | null>
  rowHeight: number
  totalPages: number
}

export function useNavigatorFileDrop({
  panelRef,
  viewportRef,
  rowHeight,
  totalPages,
}: UseNavigatorFileDropOptions) {
  const [isDragging, setIsDragging] = useState(false)
  const [insertIndex, setInsertIndex] = useState<number | null>(null)
  const actorRef = useProcessingActorRef()
  const dragCounterRef = useRef(0)
  const tauriDropHandledRef = useRef(false)

  const totalPagesRef = useRef(totalPages)
  totalPagesRef.current = totalPages
  const insertIndexRef = useRef<number | null>(null)
  insertIndexRef.current = insertIndex

  // ── Shared helpers ──────────────────────────────────────────────

  const computeInsertIndex = useCallback(
    (clientX: number, clientY: number): number | null => {
      const panel = panelRef.current
      const viewport = viewportRef.current
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
      const idx = Math.round(relativeY / rowHeight)
      return Math.max(0, Math.min(idx, totalPagesRef.current))
    },
    [panelRef, viewportRef, rowHeight],
  )

  // ── Auto-scroll when cursor near viewport edges ────────────────
  const EDGE_ZONE = 40
  const SCROLL_SPEED = 8
  const scrollRafRef = useRef<number | null>(null)
  const lastCursorYRef = useRef<number | null>(null)

  const startAutoScroll = useCallback(() => {
    const tick = () => {
      const viewport = viewportRef.current
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
  }, [viewportRef])

  const stopAutoScroll = useCallback(() => {
    if (scrollRafRef.current !== null) {
      cancelAnimationFrame(scrollRafRef.current)
      scrollRafRef.current = null
    }
    lastCursorYRef.current = null
  }, [])

  // ── Tauri native DnD ──────────────────────────────────────────
  useEffect(() => {
    if (!isTauri()) return

    let cancelled = false
    let unlisten: (() => void) | null = null

    const setup = async () => {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview')
      if (cancelled) return

      unlisten = await getCurrentWebview().onDragDropEvent(
        (event: { payload: DragDropPayload }) => {
          const payload = event.payload

          if (payload.type === 'enter') {
            const idx = computeInsertIndex(
              payload.position.x,
              payload.position.y,
            )
            if (idx !== null) {
              setIsDragging(true)
              setInsertIndex(idx)
              lastCursorYRef.current = payload.position.y
              startAutoScroll()
            }
          } else if (payload.type === 'over') {
            const idx = computeInsertIndex(
              payload.position.x,
              payload.position.y,
            )
            if (idx !== null) {
              setIsDragging(true)
              setInsertIndex(idx)
              lastCursorYRef.current = payload.position.y
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
            const idx = computeInsertIndex(
              payload.position.x,
              payload.position.y,
            )
            dragCounterRef.current = 0
            setIsDragging(false)
            setInsertIndex(null)
            stopAutoScroll()

            if (idx === null) return

            if (payload.paths.length === 0) {
              showError('dropZone.noFilePaths')
              return
            }

            tauriDropHandledRef.current = true
            setTimeout(() => {
              tauriDropHandledRef.current = false
            }, 500)

            actorRef.send({
              type: 'START_DROP_IMPORT_PATHS',
              paths: payload.paths,
              mode: 'append',
              insertAt: idx,
            })
          }
        },
      )
    }

    setup()
    return () => {
      cancelled = true
      unlisten?.()
      stopAutoScroll()
    }
  }, [actorRef, computeInsertIndex, startAutoScroll, stopAutoScroll])

  // ── HTML5 DnD (always active — handles browser image drops) ───
  const onDragEnter = useCallback(
    (e: DragEvent) => {
      e.preventDefault()
      e.stopPropagation()
      dragCounterRef.current += 1
      if (dragCounterRef.current === 1) {
        setIsDragging(true)
        const idx = computeInsertIndex(e.clientX, e.clientY)
        setInsertIndex(idx)
        lastCursorYRef.current = e.clientY
        startAutoScroll()
      }
    },
    [computeInsertIndex, startAutoScroll],
  )

  const onDragOver = useCallback(
    (e: DragEvent) => {
      e.preventDefault()
      e.stopPropagation()
      const dt = e.dataTransfer
      if (dt) {
        const types = dt.types
        if (
          types.includes('Files') ||
          types.includes('text/uri-list') ||
          types.includes('text/html')
        ) {
          dt.dropEffect = 'copy'
        }
      }
      const idx = computeInsertIndex(e.clientX, e.clientY)
      setInsertIndex(idx)
      lastCursorYRef.current = e.clientY
    },
    [computeInsertIndex],
  )

  const onDragLeave = useCallback(
    (e: DragEvent) => {
      e.preventDefault()
      e.stopPropagation()
      dragCounterRef.current -= 1
      if (dragCounterRef.current === 0) {
        setIsDragging(false)
        setInsertIndex(null)
        stopAutoScroll()
      }
    },
    [stopAutoScroll],
  )

  const onDrop = useCallback(
    (e: DragEvent) => {
      e.preventDefault()
      e.stopPropagation()
      dragCounterRef.current = 0
      setIsDragging(false)
      setInsertIndex(null)
      stopAutoScroll()

      if (tauriDropHandledRef.current) {
        tauriDropHandledRef.current = false
        return
      }

      const dt = e.dataTransfer
      if (!dt) return

      const insertAt = insertIndexRef.current ?? totalPagesRef.current

      // 1. File objects (file-system drops in browsers, Safari image drags).
      const imageFiles = Array.from(dt.files).filter(isAllowedImageFile)
      if (imageFiles.length > 0) {
        actorRef.send({
          type: 'START_DROP_IMPORT',
          files: imageFiles,
          mode: 'append',
          insertAt,
        })
        return
      }

      // 2. URL drops (Chrome / Firefox image drags from a webpage).
      //    The backend fetches the URLs server-side (no CORS restriction).
      const uriList = dt.getData('text/uri-list')
      if (uriList) {
        const urls = extractImageUrls(uriList)
        if (urls.length > 0) {
          actorRef.send({
            type: 'START_DROP_IMPORT_PATHS',
            paths: urls,
            mode: 'append',
            insertAt,
          })
          return
        }
      }

      showError('dropZone.noValidFiles')
    },
    [actorRef, stopAutoScroll],
  )

  useEffect(() => {
    const el = panelRef.current
    if (!el) return

    el.addEventListener('dragenter', onDragEnter)
    el.addEventListener('dragover', onDragOver)
    el.addEventListener('dragleave', onDragLeave)
    el.addEventListener('drop', onDrop)

    return () => {
      el.removeEventListener('dragenter', onDragEnter)
      el.removeEventListener('dragover', onDragOver)
      el.removeEventListener('dragleave', onDragLeave)
      el.removeEventListener('drop', onDrop)
    }
  }, [panelRef, onDragEnter, onDragOver, onDragLeave, onDrop])

  useEffect(() => {
    const prevent = (e: DragEvent) => {
      e.preventDefault()
    }
    window.addEventListener('dragover', prevent)
    window.addEventListener('drop', prevent)
    return () => {
      window.removeEventListener('dragover', prevent)
      window.removeEventListener('drop', prevent)
    }
  }, [])

  return { isDragging, insertIndex }
}
