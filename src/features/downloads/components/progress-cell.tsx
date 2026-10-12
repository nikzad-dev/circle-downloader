import type { DownloadTask } from '@/lib/orpc/schemas'
import { Progress } from '@/components/ui/progress'
import { formatBytes, formatSpeed } from '../lib/format'

export function ProgressCell({
  task,
  compact = false,
}: {
  task: DownloadTask
  compact?: boolean
}) {
  const percent =
    task.totalBytes !== null && task.totalBytes > 0
      ? Math.min(
          100,
          Math.round((task.downloadedBytes / task.totalBytes) * 100),
        )
      : null
  const value = task.status === 'completed' ? 100 : percent
  if (compact)
    return (
      <div
        className="flex items-center gap-2"
        data-progress-status={task.status}
        title={`${formatBytes(task.downloadedBytes)} / ${formatBytes(task.totalBytes)}`}
      >
        <Progress
          aria-label={`Progress for ${task.fileName}`}
          value={value}
          className="h-1.5 flex-1"
        />
        <span className="w-8 text-right text-xs tabular-nums">
          {value === null ? '—' : `${value}%`}
        </span>
      </div>
    )
  return (
    <div className="min-w-28 space-y-1">
      <div className="flex items-center justify-between gap-2 text-xs tabular-nums">
        <span className="text-muted-foreground">
          {formatBytes(task.downloadedBytes)}
          {task.totalBytes !== null && ` / ${formatBytes(task.totalBytes)}`}
        </span>
        <span>
          {task.status === 'verifying'
            ? 'Verifying'
            : value === null
              ? '—'
              : `${value}%`}
        </span>
      </div>
      <Progress
        aria-label={`Progress for ${task.fileName}`}
        value={value}
        className="w-full"
      />
      <span className="block text-xs text-muted-foreground tabular-nums">
        {task.status === 'downloading'
          ? formatSpeed(task.speedBytesPerSecond)
          : '—'}
      </span>
    </div>
  )
}
