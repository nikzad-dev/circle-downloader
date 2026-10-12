import type { QueuePolicy } from './api'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import {
  Copy,
  Gauge,
  Layers3,
  Loader2,
  Pause,
  Play,
  Plus,
  Settings2,
} from 'lucide-react'
import { useId, useState } from 'react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { downloadsKey } from '@/features/downloads/api/downloads'
import { advanced, advancedKeys, useQueues } from './api'

const bandwidthUnits = {
  'KB/s': 1_000,
  'MB/s': 1_000_000,
  'GB/s': 1_000_000_000,
} as const
type BandwidthUnit = keyof typeof bandwidthUnits
function bandwidthUnit(bytes: number): BandwidthUnit {
  return bytes >= bandwidthUnits['GB/s']
    ? 'GB/s'
    : bytes >= bandwidthUnits['MB/s']
      ? 'MB/s'
      : 'KB/s'
}

const newQueue: QueuePolicy = {
  id: '',
  name: '',
  paused: false,
  maxConcurrent: 3,
  bytesPerSecond: 0,
  taskCount: 0,
}

export function QueuesPage() {
  const queues = useQueues()
  const [editing, setEditing] = useState<QueuePolicy | null>(null)
  const [notice, setNotice] = useState('')
  return (
    <section aria-label="Queues" className="w-full space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-2">
          <h1 className="text-2xl font-semibold tracking-tight">Queues</h1>
          <p className="max-w-xl text-sm text-muted-foreground">
            Give each queue its own pace. Set simultaneous transfers and
            bandwidth limits in one place.
          </p>
        </div>
        <Button onClick={() => setEditing({ ...newQueue })}>
          <Plus />
          Create queue
        </Button>
      </div>
      {notice && (
        <p role="status" className="text-sm text-muted-foreground">
          {notice}
        </p>
      )}
      {queues.isError ? (
        <div
          role="alert"
          className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-destructive/30 p-5"
        >
          <p className="text-sm">Could not load queues. Try again.</p>
          <Button variant="outline" onClick={() => void queues.refetch()}>
            Retry queues
          </Button>
        </div>
      ) : queues.isPending ? (
        <div
          role="status"
          className="rounded-xl border border-dashed p-10 text-center text-sm text-muted-foreground"
        >
          Loading queues…
        </div>
      ) : queues.data?.length === 0 ? (
        <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed px-6 py-14 text-center">
          <Layers3 className="size-8 text-muted-foreground" />
          <h2 className="text-lg font-semibold">A pace for every project</h2>
          <p className="max-w-sm text-sm text-muted-foreground">
            Create your first queue to keep different transfer limits organized.
          </p>
          <Button className="mt-2" onClick={() => setEditing({ ...newQueue })}>
            <Plus />
            Create queue
          </Button>
        </div>
      ) : (
        <div className="grid items-start gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {queues.data?.map((queue) => (
            <QueueCard
              key={queue.id}
              queue={queue}
              onEdit={() => setEditing({ ...queue })}
              onDuplicate={() =>
                setEditing({
                  ...queue,
                  id: '',
                  name: `${queue.name.slice(0, 73)} (copy)`,
                  taskCount: 0,
                })
              }
              onNotice={setNotice}
            />
          ))}
        </div>
      )}
      <p className="flex items-start gap-2 text-xs leading-relaxed text-muted-foreground">
        <Gauge className="mt-0.5 size-4 shrink-0" />
        Queue limits work within your global transfer and bandwidth limits.
        Pausing stops active transfers and holds waiting files until you resume.
        Files already verifying can finish.
      </p>
      <Dialog
        open={editing !== null}
        onOpenChange={(open) => {
          if (!open) setEditing(null)
        }}
      >
        {editing && (
          <QueueEditor
            key={editing.id}
            initial={editing}
            onCancel={() => setEditing(null)}
            onSaved={(name) => {
              setEditing(null)
              setNotice(`${name} ${editing.id ? 'updated' : 'created'}.`)
            }}
          />
        )}
      </Dialog>
    </section>
  )
}

function QueueCard({
  queue,
  onEdit,
  onDuplicate,
  onNotice,
}: {
  queue: QueuePolicy
  onEdit: () => void
  onDuplicate: () => void
  onNotice: (message: string) => void
}) {
  const client = useQueryClient()
  const toggle = useMutation({
    onMutate: async () => {
      await client.cancelQueries({ queryKey: advancedKeys.queues })
    },
    mutationFn: () => advanced.saveQueue({ ...queue, paused: !queue.paused }),
    onSuccess: async (saved) => {
      client.setQueryData<QueuePolicy[]>(advancedKeys.queues, (current) =>
        current?.map((item) =>
          item.id === saved.id ? { ...item, ...saved } : item,
        ),
      )
      onNotice(
        `${saved.name} ${saved.paused ? 'paused. Transfers are stopped until you resume.' : 'resumed.'}`,
      )
      await Promise.all([
        client.invalidateQueries({ queryKey: advancedKeys.queues }),
        client.invalidateQueries({ queryKey: downloadsKey }),
      ])
    },
  })
  return (
    <article
      aria-label={queue.name}
      className="overflow-hidden rounded-lg border bg-card text-card-foreground"
    >
      <div className="flex items-start gap-2.5 px-3.5 pt-3.5">
        <div className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted">
          <Layers3 className="size-4 text-muted-foreground" />
        </div>
        <div className="min-w-0 flex-1">
          <h2 className="break-words text-sm font-semibold leading-5">
            {queue.name}
          </h2>
          <p className="mt-0.5 flex items-center gap-1.5 text-[11px] text-muted-foreground">
            {queue.paused ? (
              <Pause className="size-3" />
            ) : (
              <span className="size-1.5 rounded-full bg-emerald-500" />
            )}
            {queue.paused ? 'Paused' : 'Ready'}
            {queue.id === 'default' && (
              <span className="ml-1 text-muted-foreground/70">· Default</span>
            )}
          </p>
        </div>
      </div>
      <dl className="grid grid-cols-2 gap-3 px-3.5 py-3">
        <div>
          <dt className="text-[11px] text-muted-foreground">
            Transfers at once
          </dt>
          <dd className="mt-0.5 text-sm font-semibold tabular-nums">
            {queue.maxConcurrent}
          </dd>
        </div>
        <div>
          <dt className="text-[11px] text-muted-foreground">Bandwidth limit</dt>
          <dd className="mt-0.5 text-sm font-semibold tabular-nums">
            {queue.bytesPerSecond === 0 ? (
              'Unlimited'
            ) : (
              <>
                {(
                  queue.bytesPerSecond /
                  bandwidthUnits[bandwidthUnit(queue.bytesPerSecond)]
                ).toLocaleString(undefined, { maximumSignificantDigits: 4 })}
                <span className="ml-1 text-[11px] font-normal text-muted-foreground">
                  {bandwidthUnit(queue.bytesPerSecond)}
                </span>
              </>
            )}
          </dd>
        </div>
      </dl>
      <div className="flex items-center gap-1 border-t px-2 py-1.5">
        {queue.taskCount > 0 && (
          <Button
            variant="ghost"
            size="xs"
            disabled={toggle.isPending}
            aria-label={`${queue.paused ? 'Resume' : 'Pause'} ${queue.name}`}
            title={
              queue.paused
                ? 'Continue this queue’s waiting transfers'
                : 'Stop active transfers and hold waiting files'
            }
            onClick={() => toggle.mutate()}
          >
            {toggle.isPending ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : queue.paused ? (
              <Play className="size-3.5" />
            ) : (
              <Pause className="size-3.5" />
            )}
            {toggle.isPending ? 'Saving…' : queue.paused ? 'Resume' : 'Pause'}
          </Button>
        )}
        <Button
          variant="ghost"
          size="xs"
          className="ml-auto"
          disabled={toggle.isPending}
          aria-label={`Edit ${queue.name}`}
          onClick={onEdit}
        >
          <Settings2 className="size-3.5" />
          Edit
        </Button>
        <Button
          variant="ghost"
          size="icon-xs"
          disabled={toggle.isPending}
          aria-label={`Duplicate ${queue.name}`}
          title="Duplicate queue settings"
          onClick={onDuplicate}
        >
          <Copy className="size-3.5" />
        </Button>
      </div>
      {toggle.isError && (
        <p
          role="alert"
          className="border-t px-3.5 py-2 text-xs text-destructive"
        >
          {toggle.error instanceof Error
            ? toggle.error.message
            : 'Could not update queue. Try again.'}
        </p>
      )}
    </article>
  )
}

function QueueEditor({
  initial,
  onCancel,
  onSaved,
}: {
  initial: QueuePolicy
  onCancel: () => void
  onSaved: (name: string) => void
}) {
  const id = useId()
  const [value, setValue] = useState(initial)
  const [limited, setLimited] = useState(initial.bytesPerSecond > 0)
  const [unit, setUnit] = useState<BandwidthUnit>(
    initial.bytesPerSecond > 0 ? bandwidthUnit(initial.bytesPerSecond) : 'MB/s',
  )
  const [rate, setRate] = useState(
    initial.bytesPerSecond > 0
      ? String(
          initial.bytesPerSecond /
            bandwidthUnits[bandwidthUnit(initial.bytesPerSecond)],
        )
      : '1',
  )
  const [workers, setWorkers] = useState(String(initial.maxConcurrent))
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  const client = useQueryClient()
  const save = async () => {
    const name = value.name.trim()
    if (!name) {
      setError('Enter a queue name.')
      return
    }
    const bytesPerSecond = limited
      ? Math.round(Number(rate) * bandwidthUnits[unit])
      : 0
    if (
      limited &&
      (!Number.isFinite(bytesPerSecond) ||
        bytesPerSecond < 1 ||
        bytesPerSecond > 1_000_000_000)
    ) {
      setError(
        'Enter a bandwidth limit greater than 0 and no more than 1 GB/s.',
      )
      return
    }
    setPending(true)
    setError('')
    try {
      const saved = await advanced.saveQueue({
        ...value,
        name,
        maxConcurrent: Number(workers),
        bytesPerSecond,
      })
      await client.invalidateQueries({ queryKey: advancedKeys.queues })
      onSaved(saved.name)
    } catch (e) {
      setError(
        e instanceof Error ? e.message : 'Could not save queue. Try again.',
      )
    } finally {
      setPending(false)
    }
  }
  return (
    <DialogContent
      className="max-h-[90dvh] overflow-y-auto rounded-xl sm:max-w-lg"
      showCloseButton={!pending}
    >
      <DialogHeader>
        <DialogTitle className="normal-case tracking-tight">
          {initial.id ? 'Edit queue' : 'Create queue'}
        </DialogTitle>
        <DialogDescription>
          {initial.id
            ? 'Adjust how this queue runs.'
            : 'Choose a name and transfer limits. You can change these anytime.'}
        </DialogDescription>
      </DialogHeader>
      <form
        className="space-y-6"
        onSubmit={(e) => {
          e.preventDefault()
          void save()
        }}
      >
        <fieldset disabled={pending} className="space-y-5">
          <div className="space-y-2">
            <Label htmlFor={`${id}-name`}>Queue name</Label>
            <Input
              id={`${id}-name`}
              value={value.name}
              maxLength={80}
              placeholder="e.g. Work, Media, Overnight"
              onChange={(e) => setValue({ ...value, name: e.target.value })}
              required
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor={`${id}-workers`}>Simultaneous transfers</Label>
            <Input
              id={`${id}-workers`}
              type="number"
              min={1}
              max={32}
              step={1}
              value={workers}
              onChange={(e) => setWorkers(e.target.value)}
              required
              aria-describedby={`${id}-workers-help`}
            />
            <p
              id={`${id}-workers-help`}
              className="text-xs text-muted-foreground"
            >
              Between 1 and 32. Start with 3 for a balanced connection.
            </p>
          </div>
          <div className="space-y-3 rounded-lg border p-4">
            <Label className="flex items-center gap-2">
              <input
                type="checkbox"
                className="size-4 accent-primary"
                checked={limited}
                onChange={(e) => setLimited(e.target.checked)}
              />
              Limit bandwidth
            </Label>
            <p className="text-xs text-muted-foreground">
              {limited
                ? 'Set the maximum speed shared by this queue.'
                : 'Use available bandwidth within your global limit.'}
            </p>
            {limited && (
              <div className="space-y-2">
                <Label htmlFor={`${id}-rate`}>Bandwidth limit</Label>
                <div className="flex gap-2">
                  <Input
                    id={`${id}-rate`}
                    className="min-w-0 flex-1 tabular-nums"
                    type="number"
                    min={1 / bandwidthUnits[unit]}
                    max={1_000_000_000 / bandwidthUnits[unit]}
                    step="any"
                    value={rate}
                    onChange={(e) => setRate(e.target.value)}
                    aria-describedby={`${id}-rate-help`}
                    required
                  />
                  <select
                    aria-label="Bandwidth unit"
                    className="h-10 rounded-md border border-input bg-background px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:opacity-50"
                    value={unit}
                    onChange={(e) => {
                      const next = e.target.value as BandwidthUnit
                      if (rate !== '' && Number.isFinite(Number(rate))) {
                        setRate(
                          String(
                            Number(
                              (
                                (Number(rate) * bandwidthUnits[unit]) /
                                bandwidthUnits[next]
                              ).toPrecision(15),
                            ),
                          ),
                        )
                      }
                      setUnit(next)
                    }}
                  >
                    {Object.keys(bandwidthUnits).map((unit) => (
                      <option key={unit} value={unit}>
                        {unit}
                      </option>
                    ))}
                  </select>
                </div>
                <p
                  id={`${id}-rate-help`}
                  className="text-xs leading-relaxed text-muted-foreground"
                >
                  Megabytes or gigabytes per second. 1 GB/s = 1,000 MB/s.
                  Maximum: 1 GB/s.
                </p>
              </div>
            )}
          </div>
          <div className="space-y-2">
            <Label className="flex items-center gap-2">
              <input
                type="checkbox"
                className="size-4 accent-primary"
                checked={value.paused}
                onChange={(e) =>
                  setValue({ ...value, paused: e.target.checked })
                }
              />
              {initial.id ? 'Pause queue' : 'Create as paused'}
            </Label>
            <p className="text-xs text-muted-foreground">
              Active transfers stop and waiting files stay queued until you
              resume. Files already verifying can finish.
            </p>
          </div>
        </fieldset>
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <DialogFooter className="border-t pt-4">
          <Button
            type="button"
            variant="outline"
            disabled={pending}
            onClick={onCancel}
          >
            Cancel
          </Button>
          <Button type="submit" disabled={pending}>
            {pending ? 'Saving…' : initial.id ? 'Save changes' : 'Create queue'}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  )
}
