import type { QueryClient } from '@tanstack/react-query'
import type { DownloadTask, ProgressEvent } from '@/lib/orpc/schemas'
import { queryOptions, useQuery, useQueryClient } from '@tanstack/react-query'
import { orpc } from '@/lib/orpc/client'

export const downloadsKey = orpc.downloads.list.key()
interface SyncState {
  removed: Set<string>
  events: Map<string, ProgressEvent>
}
const syncStates = new WeakMap<QueryClient, SyncState>()
export function syncState(client: QueryClient) {
  let state = syncStates.get(client)
  if (!state) {
    state = { removed: new Set(), events: new Map() }
    syncStates.set(client, state)
  }
  return state
}
export function patchProgress(
  task: DownloadTask,
  event: ProgressEvent,
): DownloadTask {
  if (event.revision <= task.revision) return task
  return {
    ...task,
    revision: event.revision,
    downloadedBytes: event.transferredBytes,
    totalBytes: event.totalBytes,
    speedBytesPerSecond: event.bytesPerSecond,
    status: event.status,
    updatedAt: event.timestamp,
    error: event.status === 'failed' ? task.error : null,
  }
}
export function mergeSnapshot(
  current: DownloadTask[],
  incoming: DownloadTask[],
  baseline: DownloadTask[],
  state: SyncState,
) {
  const before = new Map(baseline.map((task) => [task.id, task.revision]))
  const cached = new Map(current.map((task) => [task.id, task]))
  const ids = new Set(incoming.map((task) => task.id))
  const rows = incoming
    .filter((task) => !state.removed.has(task.id))
    .map((task) => {
      const old = cached.get(task.id)
      let merged = old && old.revision > task.revision ? old : task
      const event = state.events.get(task.id)
      if (event) {
        merged = patchProgress(merged, event)
        if (task.revision >= event.revision) state.events.delete(task.id)
      }
      return merged
    })
  // Preserve creates/progress that arrived while this snapshot was in flight.
  for (const task of current) {
    if (
      !ids.has(task.id) &&
      !state.removed.has(task.id) &&
      (!before.has(task.id) || task.revision > before.get(task.id)!)
    )
      rows.push(task)
  }
  return rows
}
export function downloadsListOptions(client: QueryClient) {
  return queryOptions({
    queryKey: downloadsKey,
    staleTime: 5_000,
    retry: 1,
    queryFn: async ({ signal }) => {
      const baseline = client.getQueryData<DownloadTask[]>(downloadsKey) ?? []
      const incoming = await orpc.downloads.list.call(undefined, { signal })
      return mergeSnapshot(
        client.getQueryData<DownloadTask[]>(downloadsKey) ?? [],
        incoming,
        baseline,
        syncState(client),
      )
    },
  })
}
export function useDownloads() {
  const client = useQueryClient()
  return useQuery(downloadsListOptions(client))
}
export function mergeTask(client: QueryClient, task: DownloadTask) {
  const state = syncState(client)
  if (state.removed.has(task.id)) return
  const event = state.events.get(task.id)
  const next = event ? patchProgress(task, event) : task
  client.setQueryData<DownloadTask[]>(downloadsKey, (rows = []) => {
    const old = rows.find((row) => row.id === task.id)
    if (old && old.revision > next.revision) return rows
    return old
      ? rows.map((row) => (row.id === next.id ? next : row))
      : [next, ...rows]
  })
}
export function removeTask(client: QueryClient, id: string) {
  const state = syncState(client)
  state.removed.add(id)
  state.events.delete(id)
  client.setQueryData<DownloadTask[]>(downloadsKey, (rows) =>
    rows?.filter((row) => row.id !== id),
  )
}
export async function reconcileDownloads(client: QueryClient) {
  return client.invalidateQueries({ queryKey: downloadsKey })
}
