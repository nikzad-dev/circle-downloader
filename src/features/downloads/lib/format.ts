import type { DownloadStatus } from '@/lib/orpc/schemas'
import { appLocale } from '@/lib/ui-strings'

const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB']
export function formatBytes(bytes: number | null) {
  if (bytes === null)
    return 'Unknown size'
  if (!Number.isFinite(bytes) || bytes <= 0)
    return '0 B'
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${new Intl.NumberFormat(appLocale(), { maximumFractionDigits: index === 0 ? 0 : 1 }).format(bytes / 1024 ** index)} ${units[index]}`
}
export function formatSpeed(bytes: number) {
  return bytes > 0 ? `${formatBytes(bytes)}/s` : '—'
}
export const statusLabels: Record<DownloadStatus, string> = {
  queued: 'Queued',
  connecting: 'Connecting',
  downloading: 'Downloading',
  paused: 'Paused',
  verifying: 'Verifying',
  completed: 'Completed',
  failed: 'Failed',
  canceled: 'Canceled',
}
export const activeStatuses = new Set<DownloadStatus>(['queued', 'connecting', 'downloading', 'verifying'])
export function taskActions(status: DownloadStatus) {
  return { pause: ['queued', 'connecting', 'downloading'].includes(status), resume: status === 'paused', cancel: !['completed', 'canceled'].includes(status), retry: status === 'failed' }
}
