import type { ReactNode } from 'react'
import type { DownloadsSearch, DownloadsSort } from '../lib/task-query'
import {
  ArrowDownWideNarrow,
  Gauge,
  Pause,
  Play,
  RotateCcw,
  Search,
  Trash2,
  X,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

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
  onClearSelection,
  children,
  speed,
  queueName,
}: {
  search: DownloadsSearch
  onChange: (next: DownloadsSearch) => void
  fileCount?: string
  selectedCount: number
  onBulk: (action: 'pause' | 'resume' | 'retry' | 'cancel' | 'remove') => void
  bulkPending: boolean
  onClearSelection?: () => void
  availableActions?: readonly string[]
  children?: ReactNode
  speed?: string
  queueName?: string
}) {
  const filters = [
    {
      key: 'category',
      label: `Category: ${search.category ?? ''}`,
      value: search.category,
    },
    { key: 'tag', label: `Tag: ${search.tag ?? ''}`, value: search.tag },
    {
      key: 'queueId',
      label: `Queue: ${queueName ?? 'Selected queue'}`,
      value: search.queueId,
    },
  ] as const
  const filtered = Boolean(search.category || search.tag || search.queueId)
  const sort =
    sorts.find((item) => item.value === (search.sort ?? 'created_asc')) ??
    sorts[0]
  return (
    <div className="shrink-0 border-b bg-background">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-3 px-5 pt-4 pb-3">
        <div className="mr-auto flex items-baseline gap-2.5">
          <h1 className="text-lg font-semibold tracking-tight">Downloads</h1>
          {fileCount && (
            <span className="text-xs text-muted-foreground tabular-nums">
              {fileCount}
            </span>
          )}
        </div>
        {speed && speed !== '—' && (
          <span
            className="flex items-center gap-1.5 text-xs text-muted-foreground tabular-nums"
            title="Combined download speed"
          >
            <Gauge className="size-3.5" aria-hidden="true" />
            {speed}
          </span>
        )}
        <div className="relative w-full sm:w-56 lg:w-64">
          <Label className="sr-only" htmlFor="downloads-search">
            Search
          </Label>
          <Search
            className="pointer-events-none absolute top-2.5 left-3 size-3.5 text-muted-foreground"
            aria-hidden="true"
          />
          <Input
            id="downloads-search"
            value={search.q ?? ''}
            placeholder="Search downloads…"
            className="h-9 rounded-md bg-muted/40 pr-8 pl-8 text-sm"
            onChange={(event) =>
              onChange({ ...search, q: event.target.value || undefined })
            }
          />
          {search.q && (
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              className="absolute top-1 right-1"
              aria-label="Clear search"
              onClick={() => onChange({ ...search, q: undefined })}
            >
              <X className="size-3.5" />
            </Button>
          )}
        </div>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                size="sm"
                className="h-9 px-2"
                aria-label={`Sort: ${sort.label}`}
              />
            }
          >
            <ArrowDownWideNarrow className="size-4" aria-hidden="true" />
            <span className="hidden sm:inline">{sort.label}</span>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="rounded-lg">
            <DropdownMenuRadioGroup
              aria-label="Sort downloads"
              value={search.sort ?? 'created_asc'}
              onValueChange={(value) => {
                if (sorts.some((item) => item.value === value))
                  onChange({ ...search, sort: value as DownloadsSort })
              }}
            >
              {sorts.map((item) => (
                <DropdownMenuRadioItem
                  key={item.value}
                  value={item.value}
                  closeOnClick
                  className="rounded-md text-sm font-normal tracking-normal normal-case"
                >
                  {item.label}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {children}
      {filtered && (
        <div
          aria-label="Applied filters"
          className="flex flex-wrap items-center gap-2 px-5 pb-3"
        >
          {filters
            .filter((filter) => filter.value)
            .map(({ key, label }) => (
              <Button
                key={key}
                type="button"
                variant="secondary"
                size="xs"
                className="max-w-full gap-2 rounded-full font-normal"
                aria-label={`Remove ${label}`}
                onClick={() => onChange({ ...search, [key]: undefined })}
              >
                <span className="truncate">{label}</span>
                <X className="size-3" aria-hidden="true" />
              </Button>
            ))}
          <Button
            type="button"
            variant="ghost"
            size="xs"
            className="text-muted-foreground"
            onClick={() =>
              onChange({
                ...search,
                q: undefined,
                category: undefined,
                tag: undefined,
                queueId: undefined,
              })
            }
          >
            Clear filters
          </Button>
        </div>
      )}
      {selectedCount > 0 && (
        <div
          className="mx-5 mb-3 flex flex-wrap items-center gap-1 rounded-lg bg-accent/60 px-2 py-1.5"
          role="toolbar"
          aria-label="Bulk download actions"
        >
          <span className="mr-2 px-1 text-xs font-medium tabular-nums">
            {selectedCount} selected
          </span>
          {(
            [
              ['pause', 'Pause', Pause],
              ['resume', 'Resume', Play],
              ['retry', 'Retry', RotateCcw],
              ['cancel', 'Cancel', X],
              ['remove', 'Remove', Trash2],
            ] as const
          )
            .filter(
              ([action]) =>
                !availableActions || availableActions.includes(action),
            )
            .map(([action, label, Icon]) => (
              <Button
                key={action}
                type="button"
                size="xs"
                variant={action === 'remove' ? 'destructive' : 'ghost'}
                disabled={bulkPending}
                onClick={() => onBulk(action)}
              >
                <Icon className="size-3.5" aria-hidden="true" />
                {label}
              </Button>
            ))}
          {onClearSelection && (
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              className="ml-auto"
              disabled={bulkPending}
              onClick={onClearSelection}
              aria-label="Clear selection"
              title="Clear selection"
            >
              <X className="size-3.5" aria-hidden="true" />
            </Button>
          )}
        </div>
      )}
    </div>
  )
}
