import { Tabs } from '@base-ui/react/tabs'
import type { ReactNode } from 'react'
import type { DownloadsSearch } from '../lib/task-query'
import { useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import { toast } from '@/components/ui/toast'
import { useQueues, useTaskOptions } from '@/features/advanced/api'
import { QueueStartNotice } from '@/features/advanced/queue-start-notice'
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
import {
  activeStatuses,
  formatSpeed,
  statusLabels,
  taskActions,
} from '../lib/format'
import { filterAndSortTasks } from '../lib/task-query'

const downloadTabs = [
  { value: 'all', label: 'All' },
  { value: 'active', label: 'Active' },
  { value: 'paused', label: 'Paused' },
  { value: 'completed', label: 'Completed' },
  { value: 'failed', label: 'Failed' },
  { value: 'canceled', label: 'Canceled' },
] as const

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
  const queues = useQueues()
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
    <section className="flex h-full min-h-0 flex-col gap-0">
      <Tabs.Root
        value={search.status ?? 'all'}
        className="flex min-h-0 flex-1 flex-col"
        onValueChange={(value) => {
          if (typeof value !== 'string') return
          setRowSelection({})
          onSearchChange({
            ...search,
            status: value as DownloadsSearch['status'],
          })
        }}
      >
        <DownloadsToolbar
          search={{
            ...search,
            status: search.status ?? 'all',
            sort: search.sort ?? 'created_asc',
          }}
          onChange={onSearchChange}
          fileCount={
            query.data
              ? `${visible.length}${visible.length !== query.data.length ? ` of ${query.data.length}` : ''} ${query.data.length === 1 ? 'file' : 'files'}`
              : undefined
          }
          selectedCount={selectedIds.length}
          onBulk={(action) => void onBulk(action)}
          bulkPending={bulkPending}
          onClearSelection={() => setRowSelection({})}
          availableActions={(
            ['pause', 'resume', 'retry', 'cancel', 'remove'] as const
          ).filter((action) =>
            (query.data ?? []).some(
              (task) =>
                rowSelection[task.id] &&
                (action === 'remove' || taskActions(task.status)[action]),
            ),
          )}
          speed={
            query.data
              ? formatSpeed(
                  query.data.reduce(
                    (sum, task) =>
                      sum +
                      (task.status === 'downloading'
                        ? task.speedBytesPerSecond
                        : 0),
                    0,
                  ),
                )
              : undefined
          }
          queueName={
            queues.data?.find((queue) => queue.id === search.queueId)?.name
          }
        >
          <Tabs.List
            aria-label="Download sections"
            className="flex shrink-0 gap-1 overflow-x-auto px-5 pb-3"
          >
            {downloadTabs.map(({ value, label }) => (
              <Tabs.Tab
                key={value}
                value={value}
                className="relative flex shrink-0 items-center gap-2 rounded-md px-3 py-1.5 text-sm font-medium text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring data-active:bg-muted data-active:text-foreground"
              >
                {label}
                <span className="text-[11px] text-muted-foreground tabular-nums">
                  {query.data?.filter(
                    (task) =>
                      value === 'all' ||
                      (value === 'active'
                        ? activeStatuses.has(task.status)
                        : task.status === value),
                  ).length ?? '—'}
                </span>
              </Tabs.Tab>
            ))}
            {search.status &&
              !downloadTabs.some((tab) => tab.value === search.status) && (
                <Tabs.Tab
                  value={search.status}
                  className="shrink-0 rounded-md bg-muted px-3 py-1.5 text-sm font-medium"
                >
                  {statusLabels[search.status as keyof typeof statusLabels]}
                </Tabs.Tab>
              )}
          </Tabs.List>
        </DownloadsToolbar>
        {queues.data
          ?.filter(
            (queue) =>
              queue.paused &&
              visible.some(
                (task) =>
                  task.status === 'queued' &&
                  (optionsById.get(task.id)?.queueId ?? 'default') === queue.id,
              ),
          )
          .map((queue) => (
            <QueueStartNotice key={queue.id} queueId={queue.id} startNow />
          ))}
        <Tabs.Panel
          value={search.status ?? 'all'}
          className="flex min-h-0 flex-1 flex-col outline-none"
        >
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
            query.data && (
              <div className="flex min-h-0 flex-1 flex-col bg-card">
                {query.data.length === 0 ? (
                  <div className="flex min-h-0 flex-1 flex-col">
                    <DownloadsEmpty />
                  </div>
                ) : visible.length ? (
                  <DownloadsTable
                    tasks={visible}
                    rowSelection={rowSelection}
                    onRowSelectionChange={setRowSelection}
                  />
                ) : (
                  <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-sm text-muted-foreground">
                    <p>No downloads match these filters.</p>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => onSearchChange({})}
                    >
                      Show all downloads
                    </Button>
                  </div>
                )}
              </div>
            )
          )}
        </Tabs.Panel>
      </Tabs.Root>
      {detailsSlot}
    </section>
  )
}
