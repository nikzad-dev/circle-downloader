import type { DownloadsSearch, DownloadsSort } from '../lib/task-query'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

const sorts: { value: DownloadsSort, label: string }[] = [
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
}: {
  search: DownloadsSearch
  onChange: (next: DownloadsSearch) => void
  fileCount?: string
  selectedCount: number
  onBulk: (action: 'pause' | 'resume' | 'retry' | 'cancel' | 'remove') => void
  bulkPending: boolean
}) {
  const dirty = Boolean(search.q) || (search.status ?? 'all') !== 'all' || (search.sort ?? 'created_asc') !== 'created_asc'
  return (
    <div className="space-y-3 border-b p-3">
      <div className="flex flex-wrap items-center gap-3">
        <div className="min-w-40 flex-1">
          <Label className="sr-only" htmlFor="downloads-search">Search</Label>
          <Input
            id="downloads-search"
            value={search.q ?? ''}
            placeholder="Search file name or URL"
            className="h-8"
            onChange={event => onChange({ ...search, q: event.target.value || undefined })}
          />
        </div>
        <div>
          <Label className="sr-only" htmlFor="downloads-sort">Sort</Label>
          <select
            id="downloads-sort"
            className="h-8 min-w-32 rounded-md border px-2 border-input bg-transparent text-sm outline-none focus-visible:border-ring"
            value={search.sort ?? 'created_asc'}
            onChange={event => onChange({ ...search, sort: event.target.value as DownloadsSort })}
          >
            {sorts.map(item => <option key={item.value} value={item.value}>{item.label}</option>)}
          </select>
        </div>
        {fileCount !== undefined && <span className="text-xs text-muted-foreground tabular-nums">{fileCount}</span>}
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
