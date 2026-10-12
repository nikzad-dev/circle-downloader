import type { DownloadTask } from '@/lib/orpc/schemas'

export function transferPercent(task: DownloadTask) {
  if (task.status === 'completed') return 100
  if (task.totalBytes === null || task.totalBytes <= 0) return null
  return Math.min(
    100,
    Math.max(0, Math.floor((task.downloadedBytes / task.totalBytes) * 100)),
  )
}
export function transferTimeLeft(task: DownloadTask) {
  if (
    task.status !== 'downloading' ||
    task.totalBytes === null ||
    task.speedBytesPerSecond <= 0
  )
    return '—'
  const seconds = Math.ceil(
    Math.max(0, task.totalBytes - task.downloadedBytes) /
      task.speedBytesPerSecond,
  )
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`
}
