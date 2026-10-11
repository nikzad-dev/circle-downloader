import type { DownloadTask } from '@/lib/orpc/schemas'
import { Link } from '@tanstack/react-router'
import { createColumnHelper, rowSelectionFeature, tableFeatures, useTable } from '@tanstack/react-table'
import { useVirtualizer } from '@tanstack/react-virtual'
import { FileDown } from 'lucide-react'
import { useEffect, useRef } from 'react'
import { Badge } from '@/components/ui/badge'
import { Checkbox } from '@/components/ui/checkbox'
import { Table, TableBody, TableCaption, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { downloadErrorMessage } from '../lib/errors'
import { statusLabels } from '../lib/format'
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
  const columns = [
    ...(showSelection
      ? [helper.display({
          id: 'select',
          header: ({ table }) => (
            <Checkbox
              aria-label="Select all downloads"
              checked={table.getIsAllRowsSelected()}
              onCheckedChange={value => table.toggleAllRowsSelected(!!value)}
            />
          ),
          cell: ({ row }) => (
            <Checkbox
              aria-label={`Select ${row.original.fileName}`}
              checked={row.getIsSelected()}
              onCheckedChange={value => row.toggleSelected(!!value)}
            />
          ),
        })]
      : []),
    helper.display({
      id: 'file',
      header: 'File',
      cell: ({ row }) => (
        <div className="flex min-w-0 items-start gap-3">
          <FileDown className="mt-0.5 hidden size-4 shrink-0 text-muted-foreground sm:block" aria-hidden="true" />
          <div className="min-w-0">
            <Link
              to="/downloads/$downloadId"
              params={{ downloadId: row.original.id }}
              className="block max-w-44 truncate font-medium underline-offset-4 hover:underline lg:max-w-80"
              title={row.original.fileName}
            >
              {row.original.fileName}
            </Link>
            <Badge variant={row.original.status === 'failed' ? 'destructive' : 'secondary'} className="mt-1 text-[10px]">{statusLabels[row.original.status]}</Badge>
            {row.original.actualSha256 !== null && (
              <div className="mt-2 max-w-64 text-xs text-muted-foreground">
                <p>{row.original.checksumResult === 'matched' ? 'SHA-256 verified' : row.original.checksumResult === 'mismatch' ? 'SHA-256 mismatch' : 'SHA-256 calculated · no expected checksum'}</p>
                <code className="block break-all select-all">{row.original.actualSha256}</code>
              </div>
            )}
            {row.original.status === 'queued' && row.original.nextRetryAt !== null && (
              <p className="mt-2 max-w-64 text-xs leading-relaxed whitespace-normal text-muted-foreground">
                Retrying at
                {' '}
                {new Date(row.original.nextRetryAt).toLocaleString()}
                {row.original.retryCount > 0 ? ` · attempt ${row.original.retryCount} of 5` : ''}
              </p>
            )}
            {row.original.status === 'queued' && row.original.nextRetryAt === null && row.original.retryCount === 0 && (
              <p className="mt-2 max-w-64 text-xs text-muted-foreground">Waiting for a free download slot</p>
            )}
            {row.original.error && <p className="mt-2 max-w-64 text-xs leading-relaxed whitespace-normal text-destructive">{downloadErrorMessage(row.original.error)}</p>}
          </div>
        </div>
      ),
    }),
    helper.display({ id: 'progress', header: 'Progress / speed', cell: ({ row }) => <ProgressCell task={row.original} /> }),
    helper.display({ id: 'actions', header: 'Actions', cell: ({ row }) => <TaskActions task={row.original} /> }),
  ]

  const table = useTable({
    features,
    columns,
    data: tasks,
    getRowId: task => task.id,
    state: showSelection ? { rowSelection: rowSelection ?? {} } : undefined,
    onRowSelectionChange: showSelection && onRowSelectionChange
      ? (updater) => {
          const current = rowSelection ?? {}
          const next = typeof updater === 'function' ? updater(current) : updater
          onRowSelectionChange(
            Object.fromEntries(Object.entries(next).filter(([, selected]) => selected)),
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
    estimateSize: () => 76,
    overscan: 8,
    enabled: virtualize,
  })

  useEffect(() => {
    if (!showSelection || !onRowSelectionChange || !rowSelection)
      return
    const valid = new Set(tasks.map(task => task.id))
    const pruned = Object.fromEntries(
      Object.entries(rowSelection).filter(([id]) => valid.has(id)),
    ) as Record<string, true>
    if (Object.keys(pruned).length !== Object.keys(rowSelection).length)
      onRowSelectionChange(pruned)
  }, [tasks, rowSelection, onRowSelectionChange, showSelection])

  const body = virtualize
    ? (
        <TableBody>
          <tr style={{ height: virtualizer.getVirtualItems()[0]?.start ?? 0 }} aria-hidden="true">
            <td colSpan={columns.length} />
          </tr>
          {virtualizer.getVirtualItems().map((item) => {
            const row = rows[item.index]
            return (
              <TableRow key={row.id} data-state={row.getIsSelected() ? 'selected' : undefined} data-index={item.index} ref={virtualizer.measureElement}>
                {row.getAllCells().map(cell => (
                  <TableCell key={cell.id} className="py-3">
                    <table.FlexRender cell={cell} />
                  </TableCell>
                ))}
              </TableRow>
            )
          })}
          <tr style={{ height: virtualizer.getTotalSize() - (virtualizer.getVirtualItems()[virtualizer.getVirtualItems().length - 1]?.end ?? 0) }} aria-hidden="true">
            <td colSpan={columns.length} />
          </tr>
        </TableBody>
      )
    : (
        <TableBody>
          {rows.map(row => (
            <TableRow key={row.id} data-state={row.getIsSelected() ? 'selected' : undefined}>
              {row.getAllCells().map(cell => (
                <TableCell key={cell.id} className="py-3">
                  <table.FlexRender cell={cell} />
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      )

  return (
    <div ref={parentRef} className={virtualize ? 'max-h-[70vh] overflow-auto' : undefined}>
      <Table className="min-w-[420px]">
        <TableCaption className="sr-only">Download status, progress and available actions</TableCaption>
        <TableHeader>
          {table.getHeaderGroups().map(group => (
            <TableRow key={group.id}>
              {group.headers.map(header => (
                <TableHead key={header.id} scope="col" className={header.column.id === 'actions' ? 'h-9 text-right' : 'h-9'}>
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
