import type { DownloadTask } from '@/lib/orpc/schemas'
import { createColumnHelper, tableFeatures, useTable } from '@tanstack/react-table'
import { FileDown } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Table, TableBody, TableCaption, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { downloadErrorMessage } from '../lib/errors'
import { statusLabels } from '../lib/format'
import { ProgressCell } from './progress-cell'
import { TaskActions } from './task-actions'

const features = tableFeatures({})
const helper = createColumnHelper<typeof features, DownloadTask>()
const columns = [
  helper.display({ id: 'file', header: 'File', cell: ({ row }) => (
    <div className="flex min-w-0 items-start gap-3">
      <FileDown className="mt-0.5 hidden size-4 shrink-0 text-muted-foreground sm:block" aria-hidden="true" />
      <div className="min-w-0">
        <p title={row.original.fileName} className="max-w-44 truncate font-medium lg:max-w-80">{row.original.fileName}</p>
        <Badge variant={row.original.status === 'failed' ? 'destructive' : 'secondary'} className="mt-2">{statusLabels[row.original.status]}</Badge>
        {row.original.actualSha256 !== null && (
          <div className="mt-2 max-w-64 text-xs text-muted-foreground">
            <p>{row.original.checksumResult === 'matched' ? 'SHA-256 verified' : row.original.checksumResult === 'mismatch' ? 'SHA-256 mismatch' : 'SHA-256 calculated · no expected checksum'}</p>
            <code className="block break-all select-all">{row.original.actualSha256}</code>
          </div>
        )}
        {row.original.error && <p className="mt-2 max-w-64 text-xs leading-relaxed whitespace-normal text-destructive">{downloadErrorMessage(row.original.error)}</p>}
      </div>
    </div>
  ) }),
  helper.display({ id: 'progress', header: 'Progress / speed', cell: ({ row }) => <ProgressCell task={row.original} /> }),
  helper.display({ id: 'actions', header: 'Actions', cell: ({ row }) => <TaskActions task={row.original} /> }),
]
export function DownloadsTable({ tasks }: { tasks: DownloadTask[] }) {
  const table = useTable({ features, columns, data: tasks, getRowId: task => task.id })
  return (
    <Table className="min-w-[420px]">
      <TableCaption className="sr-only">Download status, progress and available actions</TableCaption>
      <TableHeader>{table.getHeaderGroups().map(group => <TableRow key={group.id}>{group.headers.map(header => <TableHead key={header.id} scope="col" className={header.column.id === 'actions' ? 'text-right' : undefined}><table.FlexRender header={header} /></TableHead>)}</TableRow>)}</TableHeader>
      <TableBody>{table.getRowModel().rows.map(row => <TableRow key={row.id}>{row.getAllCells().map(cell => <TableCell key={cell.id} className="py-5"><table.FlexRender cell={cell} /></TableCell>)}</TableRow>)}</TableBody>
    </Table>
  )
}
