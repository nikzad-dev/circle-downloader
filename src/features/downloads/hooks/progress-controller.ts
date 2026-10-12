import type { QueryClient } from '@tanstack/react-query'
import type { DownloadTask, ProgressEvent } from '@/lib/orpc/schemas'
import {
  downloadsKey,
  downloadsListOptions,
  patchProgress,
  syncState,
} from '../api/downloads'

/**
 Coalesce bytes at 10 Hz
status transitions are flushed immediately.
 */
export function createProgressController(
  client: QueryClient,
  onSyncError: () => void = () => {},
) {
  const state = syncState(client)
  const pending = new Map<string, ProgressEvent>()
  const seen = new Map<string, number>()
  let flushTimer: ReturnType<typeof setTimeout> | undefined
  let syncTimer: ReturnType<typeof setTimeout> | undefined
  let syncing = false
  let syncAgain = false
  let disposed = false
  const reconcile = () => {
    if (disposed || syncTimer) return
    if (syncing) {
      syncAgain = true
      return
    }
    syncTimer = setTimeout(() => {
      syncTimer = undefined
      if (disposed) return
      syncing = true
      void client
        .fetchQuery({ ...downloadsListOptions(client), staleTime: 0 })
        .catch(onSyncError)
        .finally(() => {
          syncing = false
          if (syncAgain) {
            syncAgain = false
            reconcile()
          }
        })
    }, 250)
  }
  const flush = () => {
    clearTimeout(flushTimer)
    flushTimer = undefined
    if (disposed || pending.size === 0) return
    const updates = new Map(pending)
    pending.clear()
    client.setQueryData<DownloadTask[]>(downloadsKey, (rows) =>
      rows?.map((task) => {
        const event = updates.get(task.id)
        return event && !state.removed.has(task.id)
          ? patchProgress(task, event)
          : task
      }),
    )
  }
  return {
    receive(event: ProgressEvent) {
      if (disposed || state.removed.has(event.taskId)) return
      const task = client
        .getQueryData<DownloadTask[]>(downloadsKey)
        ?.find((row) => row.id === event.taskId)
      const revision = Math.max(
        seen.get(event.taskId) ?? 0,
        task?.revision ?? 0,
      )
      if (event.revision <= revision) return
      seen.set(event.taskId, event.revision)
      state.events.set(event.taskId, event)
      pending.set(event.taskId, event)
      if (!task || event.revision > revision + 1 || event.status === 'failed')
        reconcile()
      if (task && task.status !== event.status) flush()
      else if (!flushTimer) flushTimer = setTimeout(flush, 100)
    },
    reconcile,
    dispose() {
      disposed = true
      clearTimeout(flushTimer)
      clearTimeout(syncTimer)
      pending.clear()
      seen.clear()
      state.events.clear()
    },
  }
}
