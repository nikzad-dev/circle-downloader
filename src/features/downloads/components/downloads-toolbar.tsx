import type { DownloadsSearch, DownloadsSort } from '../lib/task-query'
import type { DownloadStatus } from '@/lib/orpc/schemas'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { downloadStatusSchema } from '@/lib/orpc/schemas'
import { statusLabels } from '../lib/format'

const sorts: { value: DownloadsSort, label: string }[] = [
  { value: 'created_asc', label: 'Oldest first' },
  { value: 'created_desc', label: 'Newest first' },
  { value: 'name_asc', label: 'Name A–Z' },
  { value: 'name_desc', label: 'Name Z–A' },
  { value: 'status_asc', label: 'Status' },
]

const statuses: Array<DownloadStatus | 'all'> = ['all', ...downloadStatusSchema.options]

export function DownloadsToolbar({
  search,
  onChange,
  selectedCount,
  onBulk,
  bulkPending,
}: {
  search: DownloadsSearch
  onChange: (next: DownloadsSearch) => void
  selectedCount: number
  onBulk: (action: 'pause' | 'resume' | 'retry' | 'cancel' | 'remove') => void
  bulkPending: boolean
}) {
  const dirty = Boolean(search.q) || (search.status ?? 'all') !== 'all' || (search.sort ?? 'created_asc') !== 'created_asc'
  return (
    <div className="space-y-4 border-b p-4">
      <div className="flex flex-wrap items-end gap-4">
        <div className="min-w-48 flex-1 space-y-2">
          <Label htmlFor="downloads-search">Search</Label>
          <Input
            id="downloads-search"
            value={search.q ?? ''}
            placeholder="File name or URL"
            onChange={event => onChange({ ...search, q: event.target.value || undefined })}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="downloads-status">Status</Label>
          <select
            id="downloads-status"
            className="h-10 min-w-36 border-b border-input bg-transparent text-sm outline-none focus-visible:border-ring"
            value={search.status ?? 'all'}
            onChange={event => onChange({ ...search, status: event.target.value as DownloadsSearch['status'] })}
          >
            {statuses.map(value => (
              <option key={value} value={value}>{value === 'all' ? 'All statuses' : statusLabels[value]}</option>
            ))}
          </select>
        </div>
        <div className="space-y-2">
          <Label htmlFor="downloads-sort">Sort</Label>
          <select
            id="downloads-sort"
            className="h-10 min-w-40 border-b border-input bg-transparent text-sm outline-none focus-visible:border-ring"
            value={search.sort ?? 'created_asc'}
            onChange={event => onChange({ ...search, sort: event.target.value as DownloadsSort })}
          >
            {sorts.map(item => <option key={item.value} value={item.value}>{item.label}</option>)}
          </select>
        </div>
        {dirty && (
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => onChange({ status: 'all', sort: 'created_asc', q: undefined })}
          >
            Clear filters
          </Button>
        )}
      </div>
      {selectedCount > 0 && (
        <div className="flex flex-wrap items-center gap-2" role="toolbar" aria-label="Bulk download actions">
          <span className="text-sm text-muted-foreground tabular-nums">
            {selectedCount}
            {' '}
            selected
          </span>
          {([
            ['pause', 'Pause'],
            ['resume', 'Resume'],
            ['retry', 'Retry'],
            ['cancel', 'Cancel'],
            ['remove', 'Remove'],
          ] as const).map(([action, label]) => (
            <Button key={action} type="button" size="sm" variant={action === 'remove' ? 'destructive' : 'outline'} disabled={bulkPending} onClick={() => onBulk(action)}>
              {label}
            </Button>
          ))}
        </div>
      )}
    </div>
  )
}
