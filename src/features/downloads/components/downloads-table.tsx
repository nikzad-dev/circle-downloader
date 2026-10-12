import type { DownloadTask } from '@/lib/orpc/schemas'
import { Link } from '@tanstack/react-router'
import {
  createColumnHelper,
  rowSelectionFeature,
  tableFeatures,
  useTable,
} from '@tanstack/react-table'
import { useVirtualizer } from '@tanstack/react-virtual'
import { cn } from 'cn'
import { FileDown } from 'lucide-react'
import { useEffect, useMemo, useRef } from 'react'
import { Badge } from '@/components/ui/badge'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { downloadErrorMessage } from '../lib/errors'
import { formatBytes, formatDate, statusLabels } from '../lib/format'
import { ProgressCell } from './progress-cell'
import { TaskActions } from './task-actions'

const features = tableFeatures({ rowSelectionFeature })
const helper = createColumnHelper<typeof features, DownloadTask>()
const VIRTUALIZE_AT = 100

export function DownloadsTable({
  tasks,
  rowSelection,
  onRowSelectionChange,
  showSelection = true,
}: {
  tasks: DownloadTask[]
  rowSelection?: Record<string, true>
  onRowSelectionChange?: (next: Record<string, true>) => void
  showSelection?: boolean
}) {
  const columns = useMemo(
    () => [
      ...(showSelection
        ? [
            helper.display({
              id: 'select',
              header: ({ table }) => (
                <div className="flex items-center justify-center">
                  <Checkbox
                    aria-label="Select all downloads"
                    checked={table.getIsAllRowsSelected()}
                    onCheckedChange={(value) =>
                      table.toggleAllRowsSelected(!!value)
                    }
                  />
                </div>
              ),
              cell: ({ row }) => (
                <div className="flex items-center justify-center">
                  <Checkbox
                    aria-label={`Select ${row.original.fileName}`}
                    checked={row.getIsSelected()}
                    onCheckedChange={(value) => row.toggleSelected(!!value)}
                  />
                </div>
              ),
            }),
          ]
        : []),
      helper.display({
        id: 'file',
        header: 'File',
        cell: ({ row }) => {
          const task = row.original
          return (
            <div className="flex min-w-0 items-start gap-2.5">
              <FileDown
                className="mt-0.5 hidden size-4 shrink-0 text-muted-foreground sm:block"
                aria-hidden="true"
              />
              <div className="min-w-0 flex-1 space-y-0.5">
                <div className="flex flex-wrap items-center gap-1.5">
                  <Link
                    to="/downloads/$downloadId"
                    params={{ downloadId: task.id }}
                    className="block max-w-44 truncate text-sm font-medium underline-offset-4 hover:underline sm:max-w-64 lg:max-w-80"
                    title={task.fileName}
                  >
                    {task.fileName}
                  </Link>
                  <Badge
                    variant={
                      task.status === 'failed' ? 'destructive' : 'secondary'
                    }
                    className="h-4 px-1.5 py-0 text-[10px] font-normal"
                  >
                    {statusLabels[task.status]}
                  </Badge>
                </div>

                {task.actualSha256 !== null && (
                  <div className="max-w-64 text-xs text-muted-foreground leading-tight">
                    <p>
                      {task.checksumResult === 'matched'
                        ? 'SHA-256 verified'
                        : task.checksumResult === 'mismatch'
                          ? 'SHA-256 mismatch'
                          : 'SHA-256 calculated · no expected checksum'}
                    </p>
                    <code className="block break-all select-all font-mono text-[11px] opacity-80">
                      {task.actualSha256}
                    </code>
                  </div>
                )}

                {task.status === 'queued' && task.nextRetryAt !== null && (
                  <p className="max-w-64 text-xs leading-tight whitespace-normal text-muted-foreground">
                    Retrying at {new Date(task.nextRetryAt).toLocaleString()}
                    {task.retryCount > 0
                      ? ` · attempt ${task.retryCount} of 5`
                      : ''}
                  </p>
                )}
                {task.status === 'queued' &&
                  task.nextRetryAt === null &&
                  task.retryCount === 0 && (
                    <p className="max-w-64 text-xs leading-tight text-muted-foreground">
                      Waiting for a free download slot
                    </p>
                  )}
                {task.error && (
                  <p className="max-w-64 text-xs leading-tight whitespace-normal text-destructive">
                    {downloadErrorMessage(task.error)}
                  </p>
                )}
              </div>
            </div>
          )
        },
      }),
      helper.display({
        id: 'progress',
        header: 'Progress / speed',
        cell: ({ row }) => <ProgressCell task={row.original} />,
      }),
      helper.display({
        id: 'size',
        header: 'Size',
        cell: ({ row }) => {
          const bytes =
            row.original.totalBytes ??
            (row.original.status === 'completed'
              ? row.original.downloadedBytes
              : null)
          return (
            <span className="text-xs text-muted-foreground tabular-nums whitespace-nowrap">
              {bytes !== null ? formatBytes(bytes) : '—'}
            </span>
          )
        },
      }),
      helper.display({
        id: 'dateAdded',
        header: 'Date added',
        cell: ({ row }) => {
          const dateStr = row.original.createdAt
          return (
            <span
              className="text-xs text-muted-foreground tabular-nums whitespace-nowrap"
              title={new Date(dateStr).toLocaleString()}
            >
              {formatDate(dateStr)}
            </span>
          )
        },
      }),

      helper.display({
        id: 'actions',
        header: 'Actions',
        cell: ({ row }) => <TaskActions task={row.original} compact />,
      }),
    ],
    [showSelection],
  )

  const table = useTable({
    features,
    columns,
    data: tasks,
    getRowId: (task) => task.id,
    state: showSelection ? { rowSelection: rowSelection ?? {} } : undefined,
    onRowSelectionChange:
      showSelection && onRowSelectionChange
        ? (updater) => {
            const current = rowSelection ?? {}
            const next =
              typeof updater === 'function' ? updater(current) : updater
            onRowSelectionChange(
              Object.fromEntries(
                Object.entries(next).filter(([, selected]) => selected),
              ),
            )
          }
        : undefined,
    enableRowSelection: showSelection,
  })

  const parentRef = useRef<HTMLDivElement>(null)
  const rows = table.getRowModel().rows
  const virtualize = rows.length >= VIRTUALIZE_AT
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 48,
    overscan: 8,
    enabled: virtualize,
  })

  useEffect(() => {
    if (!showSelection || !onRowSelectionChange || !rowSelection) return
    const valid = new Set(tasks.map((task) => task.id))
    const pruned = Object.fromEntries(
      Object.entries(rowSelection).filter(([id]) => valid.has(id)),
    ) as Record<string, true>
    if (Object.keys(pruned).length !== Object.keys(rowSelection).length)
      onRowSelectionChange(pruned)
  }, [tasks, rowSelection, onRowSelectionChange, showSelection])

  const body =
    rows.length === 0 ? (
      <TableBody>
        <TableRow>
          <TableCell
            colSpan={columns.length}
            className="h-16 text-center text-xs text-muted-foreground"
          >
            No downloads
          </TableCell>
        </TableRow>
      </TableBody>
    ) : virtualize ? (
      <TableBody>
        <tr
          style={{ height: virtualizer.getVirtualItems()[0]?.start ?? 0 }}
          aria-hidden="true"
        >
          <td colSpan={columns.length} />
        </tr>
        {virtualizer.getVirtualItems().map((item) => {
          const row = rows[item.index]
          return (
            <TableRow
              key={row.id}
              data-state={row.getIsSelected() ? 'selected' : undefined}
              data-index={item.index}
              ref={virtualizer.measureElement}
              className="hover:bg-muted/40 transition-colors"
            >
              {row.getAllCells().map((cell) => (
                <TableCell
                  key={cell.id}
                  className={cn(
                    'py-1.5 px-2.5',
                    cell.column.id === 'actions' && 'text-right',
                    cell.column.id === 'size' && 'text-right',
                    cell.column.id === 'select' && 'w-10 px-2 text-center',
                  )}
                >
                  <table.FlexRender cell={cell} />
                </TableCell>
              ))}
            </TableRow>
          )
        })}
        <tr
          style={{
            height:
              virtualizer.getTotalSize() -
              (virtualizer.getVirtualItems()[
                virtualizer.getVirtualItems().length - 1
              ]?.end ?? 0),
          }}
          aria-hidden="true"
        >
          <td colSpan={columns.length} />
        </tr>
      </TableBody>
    ) : (
      <TableBody>
        {rows.map((row) => (
          <TableRow
            key={row.id}
            data-state={row.getIsSelected() ? 'selected' : undefined}
            className="hover:bg-muted/40 transition-colors"
          >
            {row.getAllCells().map((cell) => (
              <TableCell
                key={cell.id}
                className={cn(
                  'py-1.5 px-2.5',
                  cell.column.id === 'actions' && 'text-right',
                  cell.column.id === 'size' && 'text-right',
                  cell.column.id === 'select' && 'w-10 px-2 text-center',
                )}
              >
                <table.FlexRender cell={cell} />
              </TableCell>
            ))}
          </TableRow>
        ))}
      </TableBody>
    )

  return (
    <div
      ref={parentRef}
      className={virtualize ? 'max-h-[70vh] overflow-auto' : undefined}
    >
      <Table className="min-w-[640px]">
        <TableCaption className="sr-only">
          Download status, progress and available actions
        </TableCaption>
        <TableHeader>
          {table.getHeaderGroups().map((group) => (
            <TableRow key={group.id} className="hover:bg-transparent">
              {group.headers.map((header) => (
                <TableHead
                  key={header.id}
                  scope="col"
                  className={cn(
                    'h-8 px-2.5 text-xs font-medium',
                    header.column.id === 'actions' && 'text-right',
                    header.column.id === 'size' && 'text-right',
                    header.column.id === 'select' && 'w-10 px-2 text-center',
                  )}
                >
                  <table.FlexRender header={header} />
                </TableHead>
              ))}
            </TableRow>
          ))}
        </TableHeader>
        {body}
      </Table>
    </div>
  )
}
