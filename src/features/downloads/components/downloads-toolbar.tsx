import type { DownloadsSearch, DownloadsSort } from '../lib/task-query'
import { Pause, Play, RotateCcw, Search, Trash2, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

const sorts: { value: DownloadsSort; label: string }[] = [
  { value: 'created_asc', label: 'Oldest first' },
  { value: 'created_desc', label: 'Newest first' },
  { value: 'name_asc', label: 'Name A–Z' },
  { value: 'name_desc', label: 'Name Z–A' },
  { value: 'status_asc', label: 'Status' },
]

export function DownloadsToolbar({
  search,
  onChange,
  fileCount,
  selectedCount,
  onBulk,
  bulkPending,
  availableActions,
}: {
  search: DownloadsSearch
  onChange: (next: DownloadsSearch) => void
  fileCount?: string
  selectedCount: number
  onBulk: (action: 'pause' | 'resume' | 'retry' | 'cancel' | 'remove') => void
  bulkPending: boolean
  availableActions?: readonly string[]
}) {
  const dirty =
    Boolean(search.q) ||
    (search.status ?? 'all') !== 'all' ||
    (search.sort ?? 'created_asc') !== 'created_asc'
  return (
    <div className="shrink-0 space-y-2 border-b bg-background px-3 py-2">
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-40 max-w-sm flex-1">
          <Label className="sr-only" htmlFor="downloads-search">
            Search
          </Label>
          <Search
            className="pointer-events-none absolute top-2 left-2.5 size-4 text-muted-foreground"
            aria-hidden="true"
          />
          <Input
            id="downloads-search"
            value={search.q ?? ''}
            placeholder="Search file name or URL"
            className="h-8 pl-8 text-xs"
            onChange={(event) =>
              onChange({ ...search, q: event.target.value || undefined })
            }
          />
        </div>
        <div>
          <Label className="sr-only" htmlFor="downloads-sort">
            Sort
          </Label>
          <select
            id="downloads-sort"
            className="h-8 min-w-32 rounded-md border px-2 border-input bg-transparent text-sm outline-none focus-visible:border-ring"
            value={search.sort ?? 'created_asc'}
            onChange={(event) =>
              onChange({ ...search, sort: event.target.value as DownloadsSort })
            }
          >
            {sorts.map((item) => (
              <option key={item.value} value={item.value}>
                {item.label}
              </option>
            ))}
          </select>
        </div>
        {fileCount !== undefined && (
          <span className="text-xs text-muted-foreground tabular-nums">
            {fileCount}
          </span>
        )}
        {dirty && (
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() =>
              onChange({ status: 'all', sort: 'created_asc', q: undefined })
            }
          >
            Clear filters
          </Button>
        )}
      </div>
      {
        <div
          className="flex flex-wrap items-center gap-2"
          role="toolbar"
          aria-label="Bulk download actions"
        >
          <span className="text-sm text-muted-foreground tabular-nums">
            {selectedCount
              ? `${selectedCount} selected`
              : 'Select downloads to'}
          </span>
          {(
            [
              ['pause', 'Pause', Pause],
              ['resume', 'Resume', Play],
              ['retry', 'Retry', RotateCcw],
              ['cancel', 'Cancel', X],
              ['remove', 'Remove', Trash2],
            ] as const
          ).map(([action, label, Icon]) => (
            <Button
              key={action}
              type="button"
              size="sm"
              variant="ghost"
              disabled={
                bulkPending ||
                selectedCount === 0 ||
                (availableActions !== undefined &&
                  !availableActions.includes(action))
              }
              onClick={() => onBulk(action)}
            >
              <Icon className="size-3.5" aria-hidden="true" />
              {label}
            </Button>
          ))}
        </div>
      }
    </div>
  )
}
