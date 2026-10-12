import type { ReactNode } from 'react'
import type { DownloadsSearch } from '../lib/task-query'
import { useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import { toast } from '@/components/ui/toast'
import { useTaskOptions } from '@/features/advanced/api'
import { orpc } from '@/lib/orpc/client'
import {
  mergeTask,
  reconcileDownloads,
  removeTask,
  useDownloads,
} from '../api/downloads'
import {
  DownloadsEmpty,
  DownloadsError,
  DownloadsLoading,
} from '../components/downloads-state'
import { DownloadsTable } from '../components/downloads-table'
import { DownloadsToolbar } from '../components/downloads-toolbar'
import { useProgressConnection } from '../hooks/progress-context'
import { taskActions } from '../lib/format'
import { filterAndSortTasks } from '../lib/task-query'

type BulkAction = 'pause' | 'resume' | 'retry' | 'cancel' | 'remove'

export function DownloadsPage({
  search,
  detailsSlot,
}: {
  search: DownloadsSearch
  detailsSlot?: ReactNode
}) {
  const query = useDownloads()
  const options = useTaskOptions()
  const optionsById = useMemo(
    () => new Map(options.data?.map((o) => [o.id, o])),
    [options.data],
  )
  const connection = useProgressConnection()
  const navigate = useNavigate()
  const client = useQueryClient()
  const [rowSelection, setRowSelection] = useState<Record<string, true>>({})
  const [bulkPending, setBulkPending] = useState(false)

  const visible = useMemo(
    () => filterAndSortTasks(query.data ?? [], search, optionsById),
    [query.data, search, optionsById],
  )
  const selectedIds = Object.keys(rowSelection).filter((id) => rowSelection[id])

  const onSearchChange = (next: DownloadsSearch) => {
    void navigate({
      to: '.',
      search: {
        ...next,
        q: next.q,
        status: next.status === 'all' ? undefined : next.status,
        sort: next.sort === 'created_asc' ? undefined : next.sort,
      },
      replace: true,
    })
  }

  const onBulk = async (action: BulkAction) => {
    if (!query.data || selectedIds.length === 0 || bulkPending) return
    setBulkPending(true)
    const byId = new Map(query.data.map((task) => [task.id, task]))
    let ok = 0
    let failed = 0
    for (const id of selectedIds) {
      const task = byId.get(id)
      if (!task) {
        failed += 1
        continue
      }
      const allowed = taskActions(task.status)
      try {
        if (action === 'remove') {
          await orpc.downloads.remove.call({ id, deletePartial: true })
          removeTask(client, id)
          ok += 1
          continue
        }
        const canRun =
          action === 'pause'
            ? allowed.pause
            : action === 'resume'
              ? allowed.resume
              : action === 'retry'
                ? allowed.retry
                : allowed.cancel
        if (!canRun) {
          failed += 1
          continue
        }
        mergeTask(client, await orpc.downloads[action].call({ id }))
        ok += 1
      } catch {
        failed += 1
      }
    }
    void reconcileDownloads(client)
    setRowSelection({})
    setBulkPending(false)
    toast.add({
      title:
        failed === 0
          ? `Updated ${ok} downloads`
          : `Updated ${ok}, ${failed} failed`,
      description:
        failed > 0
          ? 'Some tasks were skipped or returned an error.'
          : undefined,
      type: failed === 0 ? 'success' : 'error',
    })
  }

  return (
    <section className="space-y-4">
      <h1 className="sr-only">Downloads</h1>
      {[search.category, search.tag, search.queueId].some(
        (value) => value !== undefined && value !== '',
      ) && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>
            {[search.category, search.tag, search.queueId]
              .filter(Boolean)
              .join(' · ')}
          </span>
          <Button
            size="sm"
            variant="ghost"
            onClick={() =>
              onSearchChange({
                ...search,
                category: undefined,
                tag: undefined,
                queueId: undefined,
              })
            }
          >
            Clear organization filter
          </Button>
        </div>
      )}
      {options.isError && (
        <div
          role="alert"
          className="flex items-center justify-between gap-3 rounded-md border p-3 text-sm"
        >
          <span>Queue, category and tag details could not be loaded.</span>
          <Button
            size="sm"
            variant="outline"
            onClick={() => void options.refetch()}
          >
            Retry organization details
          </Button>
        </div>
      )}
      {connection.unavailable && (
        <div
          role="status"
          className="flex flex-wrap items-center justify-between gap-3 border bg-muted/40 p-3 text-sm"
        >
          <span>Live updates unavailable. Refresh to check progress.</span>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              connection.reconnect()
              void query.refetch()
            }}
          >
            Reconnect
          </Button>
        </div>
      )}
      {query.isError && (
        <DownloadsError
          retry={() => void query.refetch()}
          pending={query.isFetching}
          background={query.data !== undefined}
        />
      )}
      {query.isPending ? (
        <DownloadsLoading />
      ) : (
        query.data &&
        (query.data.length ? (
          <div className="border bg-card">
            <DownloadsToolbar
              search={{
                ...search,
                q: search.q,
                status: search.status ?? 'all',
                sort: search.sort ?? 'created_asc',
              }}
              onChange={onSearchChange}
              fileCount={`${visible.length}${visible.length !== query.data.length ? ` of ${query.data.length}` : ''} ${query.data.length === 1 ? 'file' : 'files'}`}
              selectedCount={selectedIds.length}
              onBulk={(action) => void onBulk(action)}
              bulkPending={bulkPending}
            />
            {visible.length ? (
              <DownloadsTable
                tasks={visible}
                rowSelection={rowSelection}
                onRowSelectionChange={setRowSelection}
              />
            ) : (
              <p className="p-6 text-sm text-muted-foreground">
                No downloads match these filters.
              </p>
            )}
          </div>
        ) : (
          <DownloadsEmpty />
        ))
      )}
      {detailsSlot}
    </section>
  )
}
