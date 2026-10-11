import type { DownloadTask } from '@/lib/orpc/schemas'
import { Progress } from '@/components/ui/progress'
import { formatBytes, formatSpeed } from '../lib/format'

export function ProgressCell({ task }: { task: DownloadTask }) {
  const percent = task.totalBytes !== null && task.totalBytes > 0 ? Math.min(100, Math.round(task.downloadedBytes / task.totalBytes * 100)) : null
  const value = task.status === 'completed' ? 100 : percent
  return (
    <div className="min-w-28 space-y-1">
      <div className="flex items-center justify-between gap-2 text-xs tabular-nums">
        <span className="text-muted-foreground">
          {formatBytes(task.downloadedBytes)}
          {task.totalBytes !== null && ` / ${formatBytes(task.totalBytes)}`}
        </span>
        <span>{task.status === 'verifying' ? 'Verifying' : value === null ? '—' : `${value}%`}</span>
      </div>
      <Progress aria-label={`Progress for ${task.fileName}`} value={value} className="w-full" />
      <span className="block text-xs text-muted-foreground tabular-nums">{task.status === 'downloading' ? formatSpeed(task.speedBytesPerSecond) : '—'}</span>
    </div>
  )
}
