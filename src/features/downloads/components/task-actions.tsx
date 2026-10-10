import type { DownloadTask } from '@/lib/orpc/schemas'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { ExternalLink, FolderOpen, Pause, Play, RotateCcw, Trash2, X } from 'lucide-react'
import { useRef, useState } from 'react'
import { AlertDialog, AlertDialogClose, AlertDialogContent, AlertDialogDescription, AlertDialogTitle } from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Label } from '@/components/ui/label'
import { toast } from '@/components/ui/toast'
import { orpc } from '@/lib/orpc/client'
import { mergeTask, reconcileDownloads, removeTask } from '../api/downloads'
import { downloadErrorMessage } from '../lib/errors'
import { taskActions } from '../lib/format'

type Action = 'pause' | 'resume' | 'cancel' | 'retry' | 'remove'
export function TaskActions({ task }: { task: DownloadTask }) {
  const client = useQueryClient()
  const [confirm, setConfirm] = useState(false)
  const [deletePartial, setDeletePartial] = useState(true)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const keepRef = useRef<HTMLButtonElement>(null)
  const lockRef = useRef(false)
  const mutation = useMutation({ mutationFn: async (action: Action) => {
    if (action === 'remove') {
      await orpc.downloads.remove.call({ id: task.id, deletePartial })
      removeTask(client, task.id)
    }
    else {
      mergeTask(client, await orpc.downloads[action].call({ id: task.id }))
    }
    return action
  } })
  const act = async (action: Action) => {
    if (lockRef.current)
      return
    lockRef.current = true
    try {
      await mutation.mutateAsync(action)
      setConfirm(false)
      toast.add({ title: ({ pause: 'Download paused', resume: 'Download resumed', cancel: 'Download canceled', retry: 'Download retrying', remove: 'Download removed' })[action], type: 'success' })
    }
    catch (error) {
      toast.add({ title: 'Action could not be completed', description: downloadErrorMessage(error), type: 'error' })
    }
    finally {
      void reconcileDownloads(client)
      lockRef.current = false
    }
  }
  const allowed = taskActions(task.status)
  const openOrReveal = async (kind: 'open' | 'reveal') => {
    try {
      await orpc.downloads[kind].call({ id: task.id })
    }
    catch (error) {
      toast.add({ title: kind === 'open' ? 'Could not open file' : 'Could not reveal folder', description: downloadErrorMessage(error), type: 'error' })
    }
  }
  return (
    <div className="flex items-center justify-end gap-1" aria-label={`Actions for ${task.fileName}`}>
      {([['pause', Pause, 'Pause'], ['resume', Play, 'Resume'], ['retry', RotateCcw, 'Retry'], ['cancel', X, 'Cancel']] as const).map(([action, Icon, label]) => allowed[action] && <Button key={action} variant="ghost" size="icon-sm" title={label} aria-label={`${label} ${task.fileName}`} disabled={mutation.isPending} onClick={() => void act(action)}><Icon aria-hidden="true" /></Button>)}
      {task.status === 'completed' && (
        <Button variant="ghost" size="icon-sm" title="Open file" aria-label={`Open ${task.fileName}`} onClick={() => void openOrReveal('open')}>
          <ExternalLink aria-hidden="true" />
        </Button>
      )}
      {['completed', 'failed', 'paused', 'canceled'].includes(task.status) && (
        <Button variant="ghost" size="icon-sm" title="Reveal folder" aria-label={`Reveal folder for ${task.fileName}`} onClick={() => void openOrReveal('reveal')}>
          <FolderOpen aria-hidden="true" />
        </Button>
      )}
      <Button
        ref={triggerRef}
        variant="ghost"
        size="icon-sm"
        title="Remove"
        aria-label={`Remove ${task.fileName}`}
        disabled={mutation.isPending}
        onClick={() => {
          mutation.reset()
          setDeletePartial(true)
          setConfirm(true)
        }}
      >
        <Trash2 aria-hidden="true" />
      </Button>
      <AlertDialog
        open={confirm}
        onOpenChange={(next) => {
          if (!mutation.isPending)
            setConfirm(next)
        }}
      >
        <AlertDialogContent className="max-h-[calc(100svh-2rem)] overflow-y-auto" initialFocus={keepRef} finalFocus={() => triggerRef.current?.isConnected ? triggerRef.current : document.getElementById('main-content')}>
          <AlertDialogTitle className="font-heading text-xl font-semibold">Remove download?</AlertDialogTitle>
          <AlertDialogDescription className="mt-3 text-sm leading-relaxed text-muted-foreground">
            This removes
            {' '}
            <span className="break-all font-medium text-foreground">{task.fileName}</span>
            {' '}
            from your list and stops it if active. Completed files stay on disk.
          </AlertDialogDescription>
          {task.status !== 'completed' && (
            <Label className="my-5 flex items-center gap-3 text-sm">
              <Checkbox checked={deletePartial} onCheckedChange={setDeletePartial} disabled={mutation.isPending} />
              Delete the partial file
            </Label>
          )}
          {mutation.isError && <p role="alert" className="mt-4 text-sm text-destructive">{downloadErrorMessage(mutation.error)}</p>}
          <div className="mt-6 flex justify-end gap-2">
            <AlertDialogClose render={<Button ref={keepRef} variant="outline" />} disabled={mutation.isPending}>Keep download</AlertDialogClose>
            <Button variant="destructive" disabled={mutation.isPending} onClick={() => void act('remove')}>{mutation.isPending ? 'Removing…' : 'Remove download'}</Button>
          </div>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
