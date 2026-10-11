import type { TaskOptions } from './api'
import type { DownloadTask } from '@/lib/orpc/schemas'
import { useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { downloadsKey } from '@/features/downloads/api/downloads'
import { advanced, advancedKeys, emptyTaskOptions, useTaskOptions } from './api'
import { categoryFromFilename } from './categories'
import { TaskOptionsForm } from './task-options-form'

export function TaskControls({ task }: { task: DownloadTask }) {
  const query = useTaskOptions()
  if (query.isPending)
    return <p className="text-xs text-muted-foreground">Loading download controls…</p>
  if (query.isError)
    return <Button variant="outline" size="sm" onClick={() => void query.refetch()}>Retry download controls</Button>
  return <Controls key={task.id} task={task} initial={query.data?.find(t => t.id === task.id) ?? { ...emptyTaskOptions, id: task.id }} />
}
function Controls({ task, initial }: { task: DownloadTask, initial: TaskOptions }) {
  const [value, setValue] = useState(initial)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  const client = useQueryClient()
  const active = ['connecting', 'downloading', 'verifying'].includes(task.status)
  const save = async () => {
    setPending(true)
    setError('')
    try {
      setValue(await advanced.updateTask(value))
      await Promise.all([client.invalidateQueries({ queryKey: advancedKeys.tasks }), client.invalidateQueries({ queryKey: downloadsKey }), client.invalidateQueries({ queryKey: advancedKeys.queues })])
    }
    catch (e) { setError(e instanceof Error ? e.message : 'Could not update download.') }
    finally { setPending(false) }
  }
  return (
    <div className="space-y-3 border-t pt-4">
      <h2 className="text-sm font-medium">Queue, organization and scheduling</h2>
      {active && <p className="text-xs text-muted-foreground">Pause this download before changing its controls.</p>}
      <TaskOptionsForm value={value} automaticCategory={categoryFromFilename(task.fileName) || 'Uncategorized'} onChange={setValue} disabled={pending || active} />
      {value.scheduledAt !== null && (
        <p className="text-xs text-muted-foreground">
          Scheduled instant:
          {new Date(value.scheduledAt).toISOString()}
        </p>
      )}
      {error !== '' && <p role="alert" className="text-xs text-destructive">{error}</p>}
      <Button size="sm" disabled={pending || active} onClick={() => void save()}>{pending ? 'Saving…' : 'Save controls'}</Button>
    </div>
  )
}
