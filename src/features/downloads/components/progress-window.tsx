import type { DownloadTask } from '@/lib/orpc/schemas'
import {
  hashKey,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  ArrowDown,
  Check,
  CheckCheck,
  CircleAlert,
  Download,
  FileDown,
  LoaderCircle,
  Pause,
  RefreshCw,
  X,
} from 'lucide-react'
import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { RollingNumber } from '@/components/ui/rolling-number'
import { toast } from '@/components/ui/toast'
import { advanced, advancedKeys, useQueues } from '@/features/advanced/api'
import { downloadsKey, downloadsListOptions } from '../api/downloads'
import { DownloadProgressProvider } from '../hooks/download-progress'
import { useProgressConnection } from '../hooks/progress-context'
import { downloadErrorMessage } from '../lib/errors'
import {
  activeStatuses,
  formatBytes,
  formatSpeed,
  statusLabels,
} from '../lib/format'
import { transferPercent, transferTimeLeft } from '../lib/transfer-summary'
import { closeProgressWindow } from './progress-window-api'
import { TaskActions } from './task-actions'

const close = () => {
  void closeProgressWindow().catch(() =>
    toast.add({ title: 'Could not close download progress', type: 'error' }),
  )
}

export function ProgressWindowHost() {
  const [visible, setVisible] = useState(false)
  const [error, setError] = useState(false)
  useEffect(() => {
    let disposed = false
    let changed = false
    let unlisten: (() => void) | undefined
    void (async () => {
      unlisten = await listen<boolean>(
        'progress-window-visibility',
        ({ payload }) => {
          if (!disposed) {
            changed = true
            setVisible(payload)
          }
        },
      )
      if (disposed) {
        unlisten()
        return
      }
      const ready = await invoke<boolean>('progress_window_ready')
      if (!disposed && !changed) setVisible(ready)
    })().catch(() => {
      if (!disposed) setError(true)
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])
  if (error)
    return (
      <p role="alert" className="p-5">
        Could not initialize download progress. Close this window and try again.
      </p>
    )
  // Hide/reopen keeps the webview warm but releases polling and the IPC channel.
  return visible ? (
    <DownloadProgressProvider>
      <ProgressWindow />
    </DownloadProgressProvider>
  ) : null
}

export function ProgressWindow() {
  const client = useQueryClient()
  const connection = useProgressConnection()
  const query = useQuery({
    ...downloadsListOptions(client),
    refetchOnMount: 'always',
    refetchOnWindowFocus: true,
    refetchInterval: connection.unavailable ? 2000 : 5000,
  })
  const [tab, setTab] = useState<'transfers' | 'completed'>('transfers')
  const [watched, setWatched] = useState<Set<string>>(
    () =>
      new Set(
        (client.getQueryData<DownloadTask[]>(downloadsKey) ?? [])
          .filter(
            (task) =>
              activeStatuses.has(task.status) || task.status === 'paused',
          )
          .map((task) => task.id),
      ),
  )
  useEffect(
    () =>
      client.getQueryCache().subscribe((event) => {
        if (
          event.type !== 'updated' ||
          event.query.queryHash !== hashKey(downloadsKey)
        )
          return
        const tasks = (event.query.state.data ?? []) as DownloadTask[]
        const ids = tasks
          .filter(
            (task) =>
              activeStatuses.has(task.status) || task.status === 'paused',
          )
          .map((task) => task.id)
        setWatched((previous) => {
          if (ids.every((id) => previous.has(id))) return previous
          return new Set([...previous, ...ids])
        })
      }),
    [client],
  )
  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !event.defaultPrevented && !event.repeat) {
        event.preventDefault()
        close()
      }
    }
    const focus = () => {
      void client.invalidateQueries({ queryKey: downloadsKey })
    }
    window.addEventListener('keydown', escape)
    window.addEventListener('focus', focus)
    return () => {
      window.removeEventListener('keydown', escape)
      window.removeEventListener('focus', focus)
    }
  }, [client])
  const tasks = query.data ?? []
  const running = tasks.filter((task) => task.status === 'downloading')
  const waiting = tasks.filter((task) =>
    ['queued', 'connecting', 'verifying'].includes(task.status),
  )
  const unfinished = tasks.filter(
    (task) => !['completed', 'canceled'].includes(task.status),
  )
  const completed = tasks
    .filter((task) => task.status === 'completed')
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
  const visible =
    tab === 'completed'
      ? completed.slice(0, 20)
      : tasks
          .filter(
            (task) =>
              unfinished.includes(task) ||
              (task.status === 'completed' && watched.has(task.id)),
          )
          .sort(
            (a, b) =>
              Number(a.status === 'completed') -
                Number(b.status === 'completed') ||
              a.createdAt.localeCompare(b.createdAt),
          )
  const totalSpeed = running.reduce(
    (sum, task) => sum + task.speedBytesPerSecond,
    0,
  )
  return (
    <main
      id="main-content"
      tabIndex={-1}
      className="flex h-svh flex-col overflow-hidden bg-background text-foreground outline-none"
    >
      <header className="flex shrink-0 items-center gap-3 border-b px-5 py-4">
        <span className="flex size-9 items-center justify-center rounded-xl bg-sidebar-accent text-sidebar-primary">
          <Download className="size-5" aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <h1 className="font-heading text-base font-semibold">
            Download progress
          </h1>
          <p className="text-xs text-muted-foreground">
            All your transfers, in one place
          </p>
        </div>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Refresh downloads"
          title="Refresh downloads"
          disabled={query.isFetching}
          onClick={() => void query.refetch()}
        >
          <RefreshCw className="size-4" aria-hidden="true" />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Close progress window"
          title="Close · Esc"
          onClick={close}
        >
          <X className="size-4" aria-hidden="true" />
        </Button>
      </header>
      <section
        aria-label="Transfer summary"
        className="shrink-0 border-b bg-sidebar/60 px-5 py-5"
      >
        <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
          <ArrowDown className="size-3.5" aria-hidden="true" />
          Combined download speed
        </div>
        <div className="mt-2 flex flex-wrap items-end justify-between gap-4">
          <p className="font-heading text-3xl font-semibold tracking-tight tabular-nums">
            {query.data
              ? totalSpeed > 0
                ? formatSpeed(totalSpeed)
                : '0 B/s'
              : '—'}
          </p>
          <div className="flex gap-5 text-xs">
            <span>
              <strong className="block text-lg font-semibold text-foreground tabular-nums">
                <RollingNumber value={running.length} />
              </strong>
              Downloading
            </span>
            <span>
              <strong className="block text-lg font-semibold text-foreground tabular-nums">
                <RollingNumber value={waiting.length} />
              </strong>
              Waiting / checking
            </span>
          </div>
        </div>
      </section>
      {connection.unavailable && (
        <div
          role="status"
          className="flex shrink-0 items-center justify-between gap-3 border-b bg-muted/50 px-5 py-2 text-xs"
        >
          <span>
            Live updates interrupted. Checking progress every 2 seconds.
          </span>
          <Button
            variant="ghost"
            size="sm"
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
        <div
          role="alert"
          className="flex shrink-0 items-center justify-between gap-3 border-b px-5 py-3 text-xs text-destructive"
        >
          <span>
            {query.data
              ? 'Could not refresh downloads. Showing the last available status.'
              : 'Could not load downloads.'}
          </span>
          <Button
            variant="outline"
            size="sm"
            disabled={query.isFetching}
            onClick={() => void query.refetch()}
          >
            Retry
          </Button>
        </div>
      )}
      <nav
        aria-label="Progress views"
        className="flex shrink-0 items-center gap-1 border-b px-5 py-2"
      >
        <Button
          size="sm"
          variant={tab === 'transfers' ? 'secondary' : 'ghost'}
          aria-pressed={tab === 'transfers'}
          onClick={() => setTab('transfers')}
        >
          Transfers{' '}
          <span className="text-muted-foreground tabular-nums">
            {unfinished.length}
          </span>
        </Button>
        <Button
          size="sm"
          variant={tab === 'completed' ? 'secondary' : 'ghost'}
          aria-pressed={tab === 'completed'}
          onClick={() => setTab('completed')}
        >
          <CheckCheck className="size-3.5" aria-hidden="true" />
          Completed
        </Button>
        {tab === 'completed' && (
          <span className="ml-auto text-[11px] text-muted-foreground">
            Latest 20
          </span>
        )}
      </nav>
      <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
        {query.isPending ? (
          <div
            role="status"
            className="flex justify-center gap-2 py-16 text-sm text-muted-foreground"
          >
            <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
            Loading transfers…
          </div>
        ) : visible.length ? (
          <div className="space-y-3">
            {visible.map((task) => (
              <TransferCard key={task.id} task={task} />
            ))}
          </div>
        ) : (
          query.data && (
            <div className="flex h-full min-h-48 flex-col items-center justify-center gap-3 text-center">
              <CheckCheck
                className="size-9 text-sidebar-primary"
                aria-hidden="true"
              />
              <h2 className="font-heading text-base font-semibold">
                {tab === 'completed'
                  ? 'No completed downloads yet'
                  : 'All caught up'}
              </h2>
              <p className="max-w-xs text-sm text-muted-foreground">
                {tab === 'completed'
                  ? 'Finished files will appear here.'
                  : 'New downloads appear here automatically. You can leave this window open while you work.'}
              </p>
            </div>
          )
        )}
      </div>
      <footer className="flex shrink-0 items-center justify-between gap-3 border-t bg-sidebar/40 px-5 py-3 text-[11px] text-muted-foreground">
        <span>
          {connection.unavailable ? 'Refreshing periodically' : 'Live progress'}
        </span>
        <span>Closing this window keeps downloads running</span>
      </footer>
    </main>
  )
}

function TransferCard({ task }: { task: DownloadTask }) {
  const client = useQueryClient()
  const queues = useQueues()
  const queue = queues.data?.find((item) => item.id === task.queueId)
  const queuePaused = task.status === 'queued' && queue?.paused
  const resumeQueue = useMutation({
    mutationFn: async () => {
      if (!queue) throw new Error('Queue is unavailable')
      await advanced.saveQueue({ ...queue, paused: false })
    },
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: advancedKeys.queues })
      void client.invalidateQueries({ queryKey: downloadsKey })
    },
    onError: (error) =>
      toast.add({
        title: 'Could not resume queue',
        description: downloadErrorMessage(error),
        type: 'error',
      }),
  })
  const percent = transferPercent(task)
  const completed = task.status === 'completed'
  const Icon = completed
    ? Check
    : task.status === 'failed'
      ? CircleAlert
      : task.status === 'paused'
        ? Pause
        : FileDown
  const statusNote =
    task.status === 'verifying'
      ? 'Checking file integrity'
      : task.nextRetryAt
        ? `Retrying at ${new Date(task.nextRetryAt).toLocaleTimeString()}`
        : task.status === 'queued'
          ? queuePaused
            ? `The ${queue.name} queue is paused`
            : 'Waiting for a download slot or scheduled start'
          : task.status === 'connecting'
            ? 'Connecting to the server'
            : null
  return (
    <article
      aria-label={task.fileName}
      className="rounded-xl border bg-card p-4 shadow-xs"
    >
      <div className="flex items-start gap-3">
        <span
          data-download-status={task.status}
          className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted/60"
        >
          <Icon className="size-4" aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-sm font-semibold" title={task.fileName}>
            {task.fileName}
          </h2>
          <p data-download-status={task.status} className="mt-1 text-xs">
            {queuePaused ? 'Queue paused' : statusLabels[task.status]}
          </p>
        </div>
        <TaskActions task={task} compact />
      </div>
      <div className="mt-4 flex items-baseline justify-between gap-3 text-xs tabular-nums">
        <span className="min-w-0 text-muted-foreground">
          {task.totalBytes === null && !completed
            ? `${formatBytes(task.downloadedBytes)} downloaded · Size pending`
            : `${formatBytes(task.downloadedBytes)} of ${formatBytes(task.totalBytes ?? task.downloadedBytes)}`}
        </span>
        {percent !== null && (
          <span className="shrink-0 font-medium">
            {task.status === 'verifying' ? 'Verifying' : `${percent}%`}
          </span>
        )}
      </div>
      <div data-progress-status={task.status} className="mt-2">
        <Progress
          aria-label={`Progress for ${task.fileName}`}
          value={
            task.status === 'queued' || task.status === 'paused'
              ? (percent ?? 0)
              : percent
          }
          className="[&_[data-slot=progress-indicator][data-indeterminate]]:w-1/3 [&_[data-slot=progress-indicator][data-indeterminate]]:animate-pulse [&_[data-slot=progress-track]]:h-2 [&_[data-slot=progress-track]]:rounded-full [&_[data-slot=progress-indicator]]:rounded-full"
        />
      </div>
      {!completed && task.status !== 'queued' && (
        <div className="mt-3 grid grid-cols-2 gap-3 text-xs">
          <div className="text-muted-foreground">
            Speed{' '}
            <span className="ml-2 font-medium text-foreground tabular-nums">
              {task.status === 'downloading'
                ? formatSpeed(task.speedBytesPerSecond)
                : '—'}
            </span>
          </div>
          <div className="text-right text-muted-foreground">
            Time left{' '}
            <span className="ml-2 font-medium text-foreground tabular-nums">
              {transferTimeLeft(task)}
            </span>
          </div>
        </div>
      )}
      {statusNote && (
        <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs text-muted-foreground">{statusNote}</p>
          {queuePaused && (
            <Button
              size="sm"
              variant="outline"
              disabled={resumeQueue.isPending}
              onClick={() => resumeQueue.mutate()}
            >
              {resumeQueue.isPending ? 'Resuming…' : 'Resume queue'}
            </Button>
          )}
        </div>
      )}
      {task.error && (
        <p className="mt-3 rounded-md bg-destructive/5 px-2.5 py-2 text-xs text-destructive">
          {downloadErrorMessage(task.error)}
        </p>
      )}
      {completed && task.checksumResult === 'matched' && (
        <p className="mt-3 text-xs text-sidebar-primary">SHA-256 verified</p>
      )}
    </article>
  )
}
