import type { DownloadTask } from '@/lib/orpc/schemas'
import { useQuery } from '@tanstack/react-query'
import { Link, useNavigate } from '@tanstack/react-router'
import { ExternalLink, FolderOpen } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Sheet, SheetContent, SheetDescription, SheetTitle } from '@/components/ui/sheet'
import { TaskControls } from '@/features/advanced/task-controls'
import { orpc } from '@/lib/orpc/client'
import { strings } from '@/lib/ui-strings'
import { downloadErrorMessage } from '../lib/errors'
import { formatBytes, statusLabels } from '../lib/format'
import { TaskActions } from './task-actions'

export function TaskDetailsSheet({ downloadId }: { downloadId: string }) {
  const navigate = useNavigate()
  const query = useQuery(orpc.downloads.get.queryOptions({ input: { id: downloadId } }))
  const close = () => {
    void navigate({ to: '/downloads', search: prev => prev })
  }

  return (
    <Sheet
      open
      onOpenChange={(open) => {
        if (!open)
          close()
      }}
    >
      <SheetContent aria-describedby="task-details-desc">
        {query.isPending && <p className="text-sm text-muted-foreground">Loading details…</p>}
        {query.isError && (
          <div className="space-y-4">
            <SheetTitle>Download not found</SheetTitle>
            <SheetDescription id="task-details-desc">This task is missing or was removed.</SheetDescription>
            <Button render={<Link to="/downloads" />} variant="outline">Back to downloads</Button>
          </div>
        )}
        {query.data && <DetailsBody task={query.data} />}
      </SheetContent>
    </Sheet>
  )
}

function DetailsBody({ task }: { task: DownloadTask }) {
  return (
    <div className="flex h-full flex-col gap-6 overflow-y-auto pr-2">
      <div className="space-y-2 pr-8">
        <SheetTitle className="break-all">{task.fileName}</SheetTitle>
        <SheetDescription id="task-details-desc">
          {statusLabels[task.status]}
          {' · '}
          {formatBytes(task.downloadedBytes)}
          {task.totalBytes != null ? ` of ${formatBytes(task.totalBytes)}` : ''}
        </SheetDescription>
      </div>
      <dl className="space-y-4 text-sm">
        <div>
          <dt className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">Source</dt>
          <dd className="mt-1 break-all">{task.url}</dd>
        </div>
        <div>
          <dt className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">Destination</dt>
          <dd className="mt-1">{task.destinationId === 'default' ? 'Downloads / CIRCLE' : task.destinationId.startsWith('circle-category:') ? `Downloads / CIRCLE / ${task.destinationId.slice('circle-category:'.length)}` : task.destinationId === 'circle-legacy' ? 'Previous downloads folder' : 'Approved download folder'}</dd>
        </div>
        <div>
          <dt className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">Transfer</dt>
          <dd className="mt-1">
            {task.transferMode === 'segmented'
              ? strings.transferModeSegmented
              : strings.transferModeSingle}
          </dd>
        </div>
        <div>
          <dt className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">Retries</dt>
          <dd className="mt-1">
            Attempt
            {' '}
            {task.retryCount}
            {' '}
            of 5
            {task.nextRetryAt !== null ? ` · next at ${new Date(task.nextRetryAt).toLocaleString()}` : ''}
          </dd>
        </div>
        {task.error && (
          <div>
            <dt className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">Error</dt>
            <dd className="mt-1 text-destructive">
              <p className="font-medium">{task.error.code}</p>
              <p className="mt-1">{downloadErrorMessage(task.error)}</p>
              {task.error.code === 'NETWORK_INTERRUPTED' && (
                <p className="mt-2 text-muted-foreground">Retry to resume safely when validators still match. Add a fresh download when resume is unsafe.</p>
              )}
            </dd>
          </div>
        )}
        {task.actualSha256 !== null && (
          <div>
            <dt className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">Verification</dt>
            <dd className="mt-1 space-y-1">
              <p>{task.checksumResult === 'matched' ? 'SHA-256 verified' : task.checksumResult === 'mismatch' ? 'SHA-256 mismatch' : 'SHA-256 calculated'}</p>
              <code className="block break-all text-xs">{task.actualSha256}</code>
            </dd>
          </div>
        )}
        <div>
          <dt className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">Timestamps</dt>
          <dd className="mt-1 text-muted-foreground">
            Created
            {' '}
            {new Date(task.createdAt).toLocaleString()}
            <br />
            Updated
            {' '}
            {new Date(task.updatedAt).toLocaleString()}
          </dd>
        </div>
      </dl>
      <TaskControls task={task} />
      <div className="mt-auto flex flex-wrap items-center gap-2 border-t pt-4">
        <TaskActions task={task} />
        {(task.status === 'completed' || task.status === 'failed' || task.status === 'paused' || task.status === 'canceled') && (
          <>
            {task.status === 'completed' && (
              <Button
                size="sm"
                variant="outline"
                onClick={() => void orpc.downloads.open.call({ id: task.id }).catch(() => {})}
              >
                <ExternalLink aria-hidden="true" />
                Open file
              </Button>
            )}
            <Button
              size="sm"
              variant="outline"
              onClick={() => void orpc.downloads.reveal.call({ id: task.id }).catch(() => {})}
            >
              <FolderOpen aria-hidden="true" />
              Reveal folder
            </Button>
          </>
        )}
      </div>
    </div>
  )
}
