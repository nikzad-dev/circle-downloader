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
import {
  FileArchive,
  FileAudio,
  FileDown,
  FileImage,
  FileText,
  FileVideo,
} from 'lucide-react'
import { useEffect, useMemo, useRef } from 'react'
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
import {
  formatBytes,
  formatDate,
  formatSpeed,
  statusLabels,
} from '../lib/format'
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
                    indeterminate={table.getIsSomeRowsSelected()}
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
            <div className="flex min-w-0 items-center gap-2">
              <FileIcon name={task.fileName} />
              <Link
                to="/downloads/$downloadId"
                params={{ downloadId: task.id }}
                className="block min-w-0 truncate text-xs font-medium underline-offset-4 hover:underline focus-visible:outline-2 focus-visible:outline-ring"
                title={task.fileName}
              >
                {task.fileName}
              </Link>
            </div>
          )
        },
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
            <span className="tabular-nums">
              {bytes !== null ? formatBytes(bytes) : '—'}
            </span>
          )
        },
      }),
      helper.display({
        id: 'status',
        header: 'Status',
        cell: ({ row }) => (
          <span
            className="inline-flex items-center gap-1.5"
            data-download-status={row.original.status}
            title={
              row.original.error
                ? downloadErrorMessage(row.original.error)
                : undefined
            }
          >
            <span
              className="status-dot size-1.5 shrink-0 rounded-full bg-current"
              aria-hidden="true"
            />
            {statusLabels[row.original.status]}
          </span>
        ),
      }),
      helper.display({
        id: 'progress',
        header: 'Progress',
        cell: ({ row }) => <ProgressCell task={row.original} compact />,
      }),
      helper.display({
        id: 'speed',
        header: 'Speed',
        cell: ({ row }) => (
          <span className="tabular-nums">
            {row.original.status === 'downloading'
              ? formatSpeed(row.original.speedBytesPerSecond)
              : '—'}
          </span>
        ),
      }),
      helper.display({
        id: 'remaining',
        header: 'Time left',
        cell: ({ row }) => (
          <span className="tabular-nums">{timeLeft(row.original)}</span>
        ),
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
        id: 'description',
        header: 'Description',
        cell: ({ row }) => {
          const task = row.original
          const description = task.error
            ? downloadErrorMessage(task.error)
            : task.actualSha256
              ? task.checksumResult === 'matched'
                ? 'SHA-256 verified'
                : task.checksumResult === 'mismatch'
                  ? 'SHA-256 mismatch'
                  : 'SHA-256 calculated · no expected checksum'
              : task.nextRetryAt
                ? `Retrying at ${new Date(task.nextRetryAt).toLocaleString()}`
                : task.status === 'queued'
                  ? 'Waiting for a free download slot'
                  : sourceHost(task.url)
          return (
            <span
              className="block truncate text-muted-foreground"
              title={[description, task.actualSha256]
                .filter(Boolean)
                .join(' · ')}
            >
              {description}
              {task.actualSha256 && (
                <span className="sr-only">{task.actualSha256}</span>
              )}
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
    estimateSize: () => 36,
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
              className="download-row"
            >
              {row.getAllCells().map((cell) => (
                <TableCell
                  key={cell.id}
                  data-column={cell.column.id}
                  className={cn(
                    'h-9 py-1 px-2.5 text-xs',
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
            className="download-row"
          >
            {row.getAllCells().map((cell) => (
              <TableCell
                key={cell.id}
                data-column={cell.column.id}
                className={cn(
                  'h-9 py-1 px-2.5 text-xs',
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
      className="downloads-grid min-h-0 flex-1 overflow-auto"
    >
      <Table className="min-w-[900px] table-fixed">
        <colgroup>
          {showSelection && <col style={{ width: 36 }} />}
          <col style={{ width: 260 }} />
          <col style={{ width: 86 }} />
          <col style={{ width: 116 }} />
          <col style={{ width: 142 }} />
          <col style={{ width: 98 }} />
          <col style={{ width: 80 }} />
          <col className="download-date-column" style={{ width: 162 }} />
          <col className="download-description-column" style={{ width: 150 }} />
          <col style={{ width: 116 }} />
        </colgroup>
        <TableCaption className="sr-only">
          Download status, progress and available actions
        </TableCaption>
        <TableHeader>
          {table.getHeaderGroups().map((group) => (
            <TableRow key={group.id} className="hover:bg-transparent">
              {group.headers.map((header) => (
                <TableHead
                  key={header.id}
                  data-column={header.column.id}
                  scope="col"
                  className={cn(
                    'h-8 px-2.5 text-xs font-medium tracking-normal normal-case',
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

function sourceHost(url: string) {
  try {
    return new URL(url).hostname
  } catch {
    return url
  }
}

function timeLeft(task: DownloadTask) {
  if (
    task.status !== 'downloading' ||
    task.totalBytes === null ||
    task.speedBytesPerSecond <= 0
  )
    return '—'
  const seconds = Math.ceil(
    Math.max(0, task.totalBytes - task.downloadedBytes) /
      task.speedBytesPerSecond,
  )
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`
}

function FileIcon({ name }: { name: string }) {
  const extension = name.split('.').pop()?.toLowerCase() ?? ''
  const Icon = /^(zip|rar|7z|gz|tar)$/.test(extension)
    ? FileArchive
    : /^(mp4|mkv|webm|mov)$/.test(extension)
      ? FileVideo
      : /^(mp3|wav|flac|ogg)$/.test(extension)
        ? FileAudio
        : /^(png|jpg|jpeg|gif|webp|svg)$/.test(extension)
          ? FileImage
          : /^(pdf|txt|doc|docx)$/.test(extension)
            ? FileText
            : FileDown
  return (
    <Icon
      className="size-4 shrink-0 text-muted-foreground"
      aria-hidden="true"
    />
  )
}
