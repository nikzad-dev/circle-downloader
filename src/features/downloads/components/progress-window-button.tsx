import { PanelsTopLeft } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { useDownloads } from '../api/downloads'
import { activeStatuses } from '../lib/format'
import { openProgressWindow } from './progress-window-api'

export function ProgressWindowButton() {
  const query = useDownloads()
  const count =
    query.data?.filter((task) => activeStatuses.has(task.status)).length ?? 0
  return (
    <Button
      variant="outline"
      size="sm"
      title={`${count} active downloads · Show all progress`}
      aria-label="Show all download progress"
      onClick={() => void openProgressWindow()}
    >
      <PanelsTopLeft className="size-4" aria-hidden="true" />
      <span className="hidden lg:inline">Progress</span>
      {count > 0 && (
        <span
          aria-label={`${count} active downloads`}
          className="rounded-sm bg-sidebar-accent px-1.5 py-0.5 text-[10px] font-semibold text-sidebar-primary tabular-nums"
        >
          {count}
        </span>
      )}
    </Button>
  )
}
