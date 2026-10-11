import type { QueuePolicy } from './api'
import type { DownloadTask } from '@/lib/orpc/schemas'
import { useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { useDownloads } from '@/features/downloads/api/downloads'
import { statusLabels } from '@/features/downloads/lib/format'
import { advanced, advancedKeys, useQueues, useTaskOptions } from './api'

export function QueuesPage() {
  const queues = useQueues()
  const downloads = useDownloads()
  const options = useTaskOptions()
  const optionsById = useMemo(() => new Map(options.data?.map(o => [o.id, o])), [options.data])
  const tasksByQueue = useMemo(() => {
    const queues = new Map<string, DownloadTask[]>()
    for (const task of downloads.data ?? []) {
      const id = optionsById.get(task.id)?.queueId ?? 'default'
      const list = queues.get(id) ?? []
      list.push(task)
      queues.set(id, list)
    }
    return queues
  }, [downloads.data, optionsById])
  return (
    <section aria-label="Queues" className="space-y-3">
      <h1 className="sr-only">Queues</h1>
      <p className="text-sm text-muted-foreground">Queues share the global worker and rate limits. Priority is aged by waiting time to prevent starvation; hosts share at most two workers.</p>
      {queues.isError && <Button variant="outline" onClick={() => void queues.refetch()}>Retry queues</Button>}
      {queues.isPending && <p>Loading queues…</p>}
      <div className="grid items-start gap-3 xl:grid-cols-2">
        {queues.data?.map(queue => (
          <Card key={queue.id} size="sm">
            <CardHeader>
              <CardTitle>
                {queue.name}
                {' '}
                <span className="font-normal text-muted-foreground">
                  ·
                  {queue.taskCount}
                  {' '}
                  unfinished
                </span>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <QueueEditor initial={queue} />
              <div className="max-h-80 overflow-auto">
                <table className="w-full text-sm">
                  <caption className="sr-only">
                    Downloads in
                    {queue.name}
                  </caption>
                  <thead>
                    <tr className="border-b text-left text-xs text-muted-foreground">
                      <th className="py-2 font-normal">File</th>
                      <th className="font-normal">State</th>
                      <th className="font-normal">Priority / schedule</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(tasksByQueue.get(queue.id) ?? []).slice(-100).reverse().map((t) => {
                      const controls = optionsById.get(t.id)
                      return (
                        <tr key={t.id} className="border-b">
                          <td className="max-w-48 truncate py-2 pr-2"><Link to="/downloads/$downloadId" params={{ downloadId: t.id }} className="hover:underline">{t.fileName}</Link></td>
                          <td className="pr-2">{statusLabels[t.status]}</td>
                          <td className="text-xs">
                            {controls?.priority ?? 0}
                            {controls?.scheduledAt != null ? ` · ${new Date(controls.scheduledAt).toLocaleString()}` : ''}
                          </td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
              </div>
              <Link to="/downloads" search={{ queueId: queue.id }} className="text-xs underline underline-offset-4">View all downloads in this queue</Link>
              <p className="text-xs text-muted-foreground">Showing up to 100 recent entries.</p>
              {(downloads.isError || options.isError) && <p role="alert" className="text-xs text-destructive">Queue membership could not be loaded.</p>}
            </CardContent>
          </Card>
        ))}
        <Card size="sm">
          <CardHeader><CardTitle>New queue</CardTitle></CardHeader>
          <CardContent><QueueEditor initial={{ id: '', name: '', paused: false, maxConcurrent: 3, bytesPerSecond: 0, taskCount: 0 }} /></CardContent>
        </Card>
      </div>
    </section>
  )
}
function QueueEditor({ initial }: { initial: QueuePolicy }) {
  const [value, setValue] = useState(initial)
  const [pending, setPending] = useState(false)
  const [message, setMessage] = useState('')
  const client = useQueryClient()
  const save = async () => {
    setPending(true)
    setMessage('')
    try {
      const saved = await advanced.saveQueue(value)
      setValue(initial.id === '' ? { ...initial } : saved)
      await client.invalidateQueries({ queryKey: advancedKeys.queues })
      setMessage('Queue saved.')
    }
    catch (e) { setMessage(e instanceof Error ? e.message : 'Could not save queue.') }
    finally { setPending(false) }
  }
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault()
        void save()
      }}
      className="space-y-3"
    >
      <fieldset disabled={pending} className="grid gap-3 sm:grid-cols-3">
        <Label className="block space-y-1">
          Name
          <Input aria-label="Queue name" value={value.name} maxLength={80} onChange={e => setValue({ ...value, name: e.target.value })} required />
        </Label>
        <Label className="block space-y-1">
          Workers
          <Input aria-label="Queue workers" type="number" min={1} max={32} value={value.maxConcurrent} onChange={e => setValue({ ...value, maxConcurrent: Number(e.target.value) })} required />
        </Label>
        <Label className="block space-y-1">
          Rate (KiB/s; 0 unlimited)
          <Input aria-label="Queue rate KiB/s" type="number" min={0} max={976562} value={value.bytesPerSecond / 1024} onChange={e => setValue({ ...value, bytesPerSecond: Math.round(Number(e.target.value) * 1024) })} required />
        </Label>
        <Label className="flex items-center gap-2 sm:col-span-3">
          <input type="checkbox" checked={value.paused} onChange={e => setValue({ ...value, paused: e.target.checked })} />
          Pause new starts (active transfers finish)
        </Label>
      </fieldset>
      {message !== '' && <p role="status" className="text-xs">{message}</p>}
      <Button type="submit" size="sm" disabled={pending}>{pending ? 'Saving…' : initial.id === '' ? 'Create queue' : 'Save queue'}</Button>
    </form>
  )
}
