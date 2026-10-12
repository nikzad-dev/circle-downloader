import type { TaskOptions } from '@/features/advanced/api'
import type { DownloadStatus, DownloadTask } from '@/lib/orpc/schemas'
import { z } from 'zod'
import { activeStatuses } from './format'
import { resolvedCategory } from '@/features/advanced/categories'
import { downloadStatusSchema } from '@/lib/orpc/schemas'

export const downloadsSortSchema = z.enum([
  'created_asc',
  'created_desc',
  'name_asc',
  'name_desc',
  'status_asc',
])

export const downloadsSearchSchema = z.object({
  q: z.string().optional().catch(undefined),
  queueId: z.string().optional().catch(undefined),
  category: z.string().optional().catch(undefined),
  tag: z.string().optional().catch(undefined),
  status: downloadStatusSchema
    .or(z.enum(['all', 'active']))
    .optional()
    .catch('all'),
  sort: downloadsSortSchema.optional().catch('created_asc'),
})

export type DownloadsSearch = z.infer<typeof downloadsSearchSchema>
export type DownloadsSort = z.infer<typeof downloadsSortSchema>

const statusRank: Record<DownloadStatus, number> = {
  downloading: 0,
  connecting: 1,
  verifying: 2,
  queued: 3,
  paused: 4,
  failed: 5,
  completed: 6,
  canceled: 7,
}

export function filterAndSortTasks(
  tasks: DownloadTask[],
  search: DownloadsSearch,
  options?: Map<string, TaskOptions>,
): DownloadTask[] {
  const q = search.q?.trim().toLowerCase() ?? ''
  const status = search.status ?? 'all'
  const sort = search.sort ?? 'created_asc'

  const filtered = tasks.filter((task) => {
    const controls = options?.get(task.id)
    if (
      search.queueId !== undefined &&
      search.queueId !== '' &&
      (controls?.queueId ?? 'default') !== search.queueId
    )
      return false
    if (
      search.category !== undefined &&
      search.category !== '' &&
      resolvedCategory(task.fileName, controls) !== search.category
    )
      return false
    if (
      search.tag !== undefined &&
      search.tag !== '' &&
      !controls?.tags.includes(search.tag)
    )
      return false
    if (
      status === 'active'
        ? !activeStatuses.has(task.status)
        : status !== 'all' && task.status !== status
    )
      return false
    if (!q) return true
    return (
      task.fileName.toLowerCase().includes(q) ||
      task.url.toLowerCase().includes(q)
    )
  })

  const sorted = [...filtered]
  sorted.sort((a, b) => {
    switch (sort) {
      case 'created_desc':
        return (
          b.createdAt.localeCompare(a.createdAt) || a.id.localeCompare(b.id)
        )
      case 'name_asc':
        return a.fileName.localeCompare(b.fileName) || a.id.localeCompare(b.id)
      case 'name_desc':
        return b.fileName.localeCompare(a.fileName) || a.id.localeCompare(b.id)
      case 'status_asc': {
        const byStatus = statusRank[a.status] - statusRank[b.status]
        return (
          byStatus ||
          a.createdAt.localeCompare(b.createdAt) ||
          a.id.localeCompare(b.id)
        )
      }
      case 'created_asc':
      default:
        return (
          a.createdAt.localeCompare(b.createdAt) || a.id.localeCompare(b.id)
        )
    }
  })
  return sorted
}

export function defaultDownloadsSearch(): DownloadsSearch {
  return { status: 'all', sort: 'created_asc' }
}
