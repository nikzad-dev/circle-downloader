import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { downloadErrorMessage } from '@/features/downloads/lib/errors'
import { advanced, advancedKeys, useQueues } from './api'

export function QueueStartNotice({
  queueId,
  startNow,
  disabled = false,
}: {
  queueId: string
  startNow: boolean
  disabled?: boolean
}) {
  const client = useQueryClient()
  const queues = useQueues()
  const queue = queues.data?.find((item) => item.id === queueId)
  const resume = useMutation({
    mutationFn: () => {
      if (!queue) throw new Error('Queue could not be loaded.')
      return advanced.saveQueue({ ...queue, paused: false })
    },
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: advancedKeys.queues })
    },
  })
  if (!startNow || !queue?.paused) return null
  return (
    <div
      role="status"
      className="space-y-2 rounded-lg border bg-muted/40 p-3 text-sm"
    >
      <p className="font-medium">{queue.name} queue is paused</p>
      <p className="text-muted-foreground">
        Downloads in this queue will wait until it is resumed. Resuming starts
        its waiting downloads.
      </p>
      <Button
        type="button"
        size="sm"
        variant="outline"
        disabled={disabled || resume.isPending}
        onClick={() => resume.mutate()}
      >
        {resume.isPending ? 'Resuming…' : 'Resume queue'}
      </Button>
      {resume.isError && (
        <p role="alert" className="text-destructive">
          {downloadErrorMessage(resume.error)}
        </p>
      )}
    </div>
  )
}
