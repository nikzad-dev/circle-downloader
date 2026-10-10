import type { CreateFormValues } from '../lib/create-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useRef, useState } from 'react'
import { Controller, useForm } from 'react-hook-form'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import { Field, FieldDescription, FieldError, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { toast } from '@/components/ui/toast'
import { orpc } from '@/lib/orpc/client'
import { mergeTask, reconcileDownloads } from '../api/downloads'
import { createFormSchema } from '../lib/create-form'
import { downloadErrorMessage } from '../lib/errors'

export function AddDownloadDialog({ open, onOpenChange, triggerRef }: { open: boolean, onOpenChange: (open: boolean) => void, triggerRef: React.RefObject<HTMLElement | null> }) {
  const [busy, setBusy] = useState(false)
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!busy)
          onOpenChange(next)
      }}
    >
      {open && <AddDownloadForm onSuccess={() => onOpenChange(false)} triggerRef={triggerRef} onBusyChange={setBusy} />}
    </Dialog>
  )
}
function AddDownloadForm({ onSuccess, triggerRef, onBusyChange }: { onSuccess: () => void, triggerRef: React.RefObject<HTMLElement | null>, onBusyChange: (busy: boolean) => void }) {
  const client = useQueryClient()
  const operationRef = useRef<{ input: string, id: string } | null>(null)
  const submittingRef = useRef(false)
  const form = useForm<CreateFormValues>({ resolver: zodResolver(createFormSchema), defaultValues: { url: '', suggestedName: '', expectedSha256: '', startNow: true } })
  const mutation = useMutation(orpc.downloads.create.mutationOptions())
  const submit = form.handleSubmit(async (values) => {
    if (submittingRef.current)
      return
    submittingRef.current = true
    onBusyChange(true)
    form.clearErrors('root')
    const input = { url: values.url, suggestedName: values.suggestedName || undefined, startNow: values.startNow, destinationId: 'default', expectedSha256: values.expectedSha256?.trim() === '' ? undefined : values.expectedSha256 }
    const signature = JSON.stringify(input)
    if (operationRef.current?.input !== signature)
      operationRef.current = { input: signature, id: crypto.randomUUID() }
    try {
      const task = await mutation.mutateAsync({ ...input, requestId: operationRef.current.id })
      mergeTask(client, task)
      void reconcileDownloads(client)
      toast.add({ title: task.status === 'paused' ? 'Download saved' : 'Download added', description: task.status === 'paused' ? 'Ready when you are. Choose Resume to start.' : 'You can follow its progress in Downloads.', type: 'success' })
      onSuccess()
    }
    catch (error) {
      const message = downloadErrorMessage(error)
      form.setError('root', { message })
      toast.add({ title: 'Could not add download', description: message, type: 'error' })
    }
    finally {
      submittingRef.current = false
      onBusyChange(false)
    }
  })
  return (
    <DialogContent className="max-h-[calc(100svh-2rem)] gap-4 overflow-y-auto" finalFocus={triggerRef} initialFocus={() => document.getElementById('download-url')}>
      <div className="space-y-2">
        <p className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">New transfer</p>
        <DialogTitle className="font-heading text-2xl font-semibold tracking-tight">Add download</DialogTitle>
        <DialogDescription className="text-sm text-muted-foreground">Paste a direct link to the file you want to save.</DialogDescription>
      </div>
      <form onSubmit={(event) => { void submit(event) }} noValidate className="space-y-4" aria-busy={mutation.isPending}>
        <Field data-invalid={!!form.formState.errors.url}>
          <FieldLabel htmlFor="download-url">Download URL</FieldLabel>
          <Input id="download-url" placeholder="https://example.com/file.zip" {...form.register('url')} aria-invalid={!!form.formState.errors.url} aria-describedby="download-url-help download-url-error" autoComplete="off" disabled={mutation.isPending} />
          <FieldDescription id="download-url-help">Use HTTPS. Local HTTP fixture links work only in debug builds.</FieldDescription>
          <FieldError id="download-url-error">{form.formState.errors.url?.message}</FieldError>
        </Field>
        <Field>
          <FieldLabel htmlFor="download-name">
            Filename
            <span className="text-muted-foreground">(optional)</span>
          </FieldLabel>
          <Input id="download-name" placeholder="Keep the name from the link" {...form.register('suggestedName')} disabled={mutation.isPending} />
        </Field>
        <Field data-invalid={!!form.formState.errors.expectedSha256}>
          <FieldLabel htmlFor="download-checksum">
            SHA-256
            <span className="text-muted-foreground">(optional)</span>
          </FieldLabel>
          <Input id="download-checksum" placeholder="64 hexadecimal characters" {...form.register('expectedSha256')} aria-invalid={!!form.formState.errors.expectedSha256} aria-describedby="download-checksum-help download-checksum-error" autoComplete="off" spellCheck={false} disabled={mutation.isPending} />
          <FieldDescription id="download-checksum-help">Paste the checksum from a trusted source to verify the downloaded file.</FieldDescription>
          <FieldError id="download-checksum-error">{form.formState.errors.expectedSha256?.message}</FieldError>
        </Field>
        <Controller
          control={form.control}
          name="startNow"
          render={({ field }) => (
            <Field orientation="horizontal">
              <Checkbox id="download-start" checked={field.value} onCheckedChange={field.onChange} disabled={mutation.isPending} />
              <div>
                <FieldLabel htmlFor="download-start">Start now</FieldLabel>
                <FieldDescription>Otherwise, save it paused and resume later. Your auto-start preference also applies.</FieldDescription>
              </div>
            </Field>
          )}
        />
        <p className="border-t pt-4 text-xs leading-relaxed text-muted-foreground">Files are saved in Circle’s app-managed download folder.</p>
        {form.formState.errors.root && <p role="alert" className="text-sm text-destructive">{form.formState.errors.root.message}</p>}
        <div className="flex justify-end gap-2">
          <DialogClose render={<Button variant="outline" />} disabled={mutation.isPending}>Cancel</DialogClose>
          <Button type="submit" disabled={mutation.isPending}>{mutation.isPending ? 'Adding…' : 'Add download'}</Button>
        </div>
      </form>
    </DialogContent>
  )
}
