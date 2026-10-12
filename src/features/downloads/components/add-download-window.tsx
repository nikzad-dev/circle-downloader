import type { ReactNode } from 'react'
import type { CreateFormValues } from '../lib/create-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useRef, useState } from 'react'
import { useForm } from 'react-hook-form'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { toast } from '@/components/ui/toast'
import { advanced, emptyTaskOptions } from '@/features/advanced/api'
import { BulkImportForm } from '@/features/advanced/bulk-import'
import { categoryFromSource } from '@/features/advanced/categories'
import { QueueStartNotice } from '@/features/advanced/queue-start-notice'
import {
  CategorySelect,
  selectClass,
  TaskOptionsForm,
} from '@/features/advanced/task-options-form'
import { orpc } from '@/lib/orpc/client'
import { createFormSchema } from '../lib/create-form'
import { downloadErrorMessage } from '../lib/errors'
import { closeDownloadWindow } from './download-window-api'
import { openProgressWindow } from './progress-window-api'

interface WindowRequest {
  initialUrl: string
  initialUrls?: string[]
  bulk: boolean
  generation: number
  requiresConfirmation?: boolean
}
export function DownloadWindowHost() {
  const client = useQueryClient()
  const [request, setRequest] = useState<WindowRequest>({
    initialUrl: '',
    bulk: false,
    generation: 0,
  })
  const [incoming, setIncoming] = useState<WindowRequest | null>(null)
  const [busy, setBusy] = useState(false)
  const newestGenerationRef = useRef(-1)
  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    const apply = (next: WindowRequest) => {
      if (disposed || next.generation <= newestGenerationRef.current) return
      newestGenerationRef.current = next.generation
      if (next.requiresConfirmation === true) {
        setIncoming(next)
        return
      }
      setIncoming(null)
      setRequest(next)
      void client.invalidateQueries()
    }
    void (async () => {
      unlisten = await listen<WindowRequest>(
        'download-window-request',
        (event) => apply(event.payload),
      )
      if (disposed) {
        unlisten()
        return
      }
      apply(await invoke<WindowRequest>('download_window_ready'))
    })().catch(() => {
      /* The main window reports failures to open. */
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [client])
  return (
    <AddDownloadWindow
      key={request.generation}
      initialUrl={request.initialUrl}
      initialUrls={request.initialUrls}
      initialBulk={request.bulk}
      onBusyChange={setBusy}
      notice={
        incoming !== null && (
          <div
            role="status"
            className="flex shrink-0 flex-wrap items-center gap-2 border-b bg-muted px-5 py-3 text-sm"
          >
            <span className="mr-auto">
              {incoming.bulk
                ? `${incoming.initialUrls?.length ?? 0} new links`
                : 'New download link'}
              . Use them instead of this draft?
            </span>
            <Button
              size="sm"
              variant="outline"
              onClick={() => setIncoming(null)}
            >
              Keep draft
            </Button>
            <Button
              size="sm"
              disabled={busy}
              onClick={() => {
                setRequest(incoming)
                setIncoming(null)
              }}
            >
              Use new links
            </Button>
          </div>
        )
      }
    />
  )
}

export function AddDownloadWindow({
  initialUrl = '',
  initialUrls = [],
  initialBulk = false,
  notice,
  onBusyChange,
}: {
  initialUrl?: string
  initialUrls?: string[]
  initialBulk?: boolean
  notice?: ReactNode
  onBusyChange?: (busy: boolean) => void
}) {
  const [bulk, setBulk] = useState(initialBulk)
  const [busy, setBusy] = useState(false)
  const closingRef = useRef(false)
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.key !== 'Escape' ||
        event.defaultPrevented ||
        event.repeat ||
        busy ||
        closingRef.current
      )
        return
      event.preventDefault()
      closingRef.current = true
      void closeDownloadWindow()
        .catch(() =>
          toast.add({ title: 'Could not close this window', type: 'error' }),
        )
        .finally(() => {
          closingRef.current = false
        })
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [busy])
  const updateBusy = (value: boolean) => {
    setBusy(value)
    onBusyChange?.(value)
  }
  return (
    <main className="download-window flex h-svh flex-col bg-background text-foreground [&_[data-slot=label]]:text-sm [&_[data-slot=label]]:font-medium [&_[data-slot=label]]:tracking-normal [&_[data-slot=label]]:normal-case [&_[data-slot=input]]:h-10 [&_[data-slot=input]]:rounded-md [&_[data-slot=input]]:border-input [&_[data-slot=input]]:px-3 [&_input[type=checkbox]]:size-4 [&_input[type=checkbox]]:shrink-0 [&_input[type=checkbox]]:accent-primary">
      <header className="flex shrink-0 items-center justify-between gap-4 border-b px-5 py-3">
        <h1 className="text-base font-semibold">New download</h1>
        <div
          role="tablist"
          aria-label="Download mode"
          onKeyDown={(e) => {
            if (
              busy ||
              !['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)
            )
              return
            e.preventDefault()
            const next =
              e.key === 'Home' ? false : e.key === 'End' ? true : !bulk
            setBulk(next)
            document
              .getElementById(next ? 'bulk-mode-tab' : 'single-mode-tab')
              ?.focus()
          }}
          className="flex rounded-lg bg-muted p-1"
        >
          <button
            type="button"
            id="single-mode-tab"
            tabIndex={bulk ? -1 : 0}
            role="tab"
            aria-controls="single-mode-panel"
            aria-selected={!bulk}
            disabled={busy}
            className={`rounded-md px-3 py-1.5 text-sm ${!bulk ? 'bg-background shadow-sm' : 'text-muted-foreground'}`}
            onClick={() => setBulk(false)}
          >
            Single URL
          </button>
          <button
            type="button"
            id="bulk-mode-tab"
            tabIndex={bulk ? 0 : -1}
            role="tab"
            aria-controls="bulk-mode-panel"
            aria-selected={bulk}
            disabled={busy}
            className={`rounded-md px-3 py-1.5 text-sm ${bulk ? 'bg-background shadow-sm' : 'text-muted-foreground'}`}
            onClick={() => setBulk(true)}
          >
            Multiple URLs
          </button>
        </div>
      </header>
      {notice}
      <div
        id="single-mode-panel"
        role="tabpanel"
        aria-labelledby="single-mode-tab"
        hidden={bulk}
        className="min-h-0 flex-1 flex-col data-[active=true]:flex"
        data-active={!bulk}
      >
        <NewDownloadForm initialUrl={initialUrl} onBusyChange={updateBusy} />
      </div>
      <div
        id="bulk-mode-panel"
        role="tabpanel"
        aria-labelledby="bulk-mode-tab"
        hidden={!bulk}
        className="min-h-0 flex-1 flex-col data-[active=true]:flex"
        data-active={bulk}
      >
        <BulkImportForm initialUrls={initialUrls} onBusyChange={updateBusy} />
      </div>
    </main>
  )
}
export function NewDownloadForm({
  initialUrl = '',
  onDone = closeDownloadWindow,
  onBusyChange,
}: {
  initialUrl?: string
  onDone?: () => Promise<void> | void
  onBusyChange?: (busy: boolean) => void
}) {
  const settings = useQuery(orpc.settings.get.queryOptions())
  const destinations = useQuery(orpc.destinations.list.queryOptions())
  const [options, setOptions] = useState(emptyTaskOptions)
  const [features, setFeatures] = useState({
    name: false,
    checksum: false,
    queue: false,
    organization: false,
    credentials: false,
    acceleration: false,
  })
  const [pending, setPending] = useState(false)
  const [added, setAdded] = useState(false)
  const inFlightRef = useRef(false)
  const operationRef = useRef<{ signature: string; id: string } | null>(null)
  const form = useForm<CreateFormValues>({
    resolver: zodResolver(createFormSchema),
    defaultValues: {
      url: initialUrl,
      suggestedName: '',
      expectedSha256: '',
      destinationId: 'default',
      startNow: true,
    },
  })
  useEffect(() => {
    if (
      settings.data?.defaultDestinationId !== undefined &&
      settings.data.defaultDestinationId !== '' &&
      !form.getFieldState('destinationId').isDirty
    )
      form.setValue('destinationId', settings.data.defaultDestinationId)
    if (settings.data !== undefined && !form.getFieldState('startNow').isDirty)
      form.setValue('startNow', settings.data.autoStart)
  }, [settings.data, form])
  const toggle = (name: keyof typeof features, label: string) => (
    <Label className="flex items-center gap-2 text-sm normal-case">
      <input
        type="checkbox"
        checked={features[name]}
        onChange={(e) => {
          setFeatures({ ...features, [name]: e.target.checked })
          if (name === 'checksum' && !e.target.checked)
            form.setValue('expectedSha256', '')
        }}
        disabled={pending || added}
      />
      {label}
    </Label>
  )
  const submit = form.handleSubmit(async (values) => {
    if (inFlightRef.current || added) return
    if (features.checksum && (values.expectedSha256?.trim() ?? '') === '') {
      form.setError('expectedSha256', {
        message: 'Enter a 64-character SHA-256 checksum.',
      })
      return
    }
    if (features.credentials && options.credentialId === null) {
      form.setError('root', {
        message: 'Choose a saved credential or turn off Use saved credentials.',
      })
      return
    }
    inFlightRef.current = true
    setPending(true)
    onBusyChange?.(true)
    form.clearErrors('root')
    const input = {
      url: values.url,
      suggestedName: features.name
        ? values.suggestedName || undefined
        : undefined,
      expectedSha256: features.checksum
        ? values.expectedSha256?.trim() === ''
          ? undefined
          : values.expectedSha256
        : undefined,
      destinationId: values.destinationId,
      startNow: values.startNow,
    }
    const controls = {
      ...options,
      queueId: features.queue ? options.queueId : 'default',
      priority: features.queue ? options.priority : 0,
      category: options.category,
      tags: features.organization ? options.tags : [],
      credentialId: features.credentials ? options.credentialId : null,
      connections: features.acceleration ? (options.connections ?? 4) : 1,
    }
    const signature = JSON.stringify({ input, controls })
    if (operationRef.current?.signature !== signature)
      operationRef.current = { signature, id: crypto.randomUUID() }
    try {
      await advanced.create(
        { ...input, requestId: operationRef.current.id },
        controls,
      )
      setAdded(true)
      await onDone()
      if (values.startNow) void openProgressWindow()
    } catch (error) {
      form.setError('root', {
        message:
          error instanceof Error &&
          'code' in error &&
          error.code === 'INVALID_TRANSITION'
            ? error.message
            : downloadErrorMessage(error),
      })
    } finally {
      inFlightRef.current = false
      setPending(false)
      onBusyChange?.(false)
    }
  })
  return (
    <form
      className="flex min-h-0 flex-1 flex-col"
      noValidate
      aria-busy={pending}
      onSubmit={(e) => {
        void submit(e)
      }}
    >
      <div className="min-h-0 flex-1 space-y-4 overflow-auto p-5">
        {(settings.isError || destinations.isError) && (
          <div role="alert" className="space-y-2 text-sm">
            <p>Download preferences could not be loaded.</p>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                void settings.refetch()
                void destinations.refetch()
              }}
            >
              Retry preferences
            </Button>
          </div>
        )}
        <fieldset disabled={pending || added} className="space-y-4">
          <section aria-label="Download source" className="space-y-2">
            <Label htmlFor="download-url">Download URL</Label>
            <Input
              id="download-url"
              autoFocus
              placeholder="https://example.com/file.zip"
              {...form.register('url')}
              autoComplete="off"
              aria-invalid={!!form.formState.errors.url}
              aria-describedby="url-error"
            />
            {form.formState.errors.url && (
              <p
                id="url-error"
                role="alert"
                className="text-sm text-destructive"
              >
                {form.formState.errors.url.message}
              </p>
            )}
          </section>
          <div className="space-y-2">
            <Label htmlFor="download-destination">Save to</Label>
            <select
              id="download-destination"
              className={selectClass}
              {...form.register('destinationId')}
            >
              {destinations.data?.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.label}
                </option>
              ))}
            </select>
          </div>
          <div className="space-y-2">
            <Label htmlFor="download-category">Category</Label>
            <CategorySelect
              id="download-category"
              value={options.category}
              onChange={(category) => setOptions({ ...options, category })}
              automaticCategory={
                categoryFromSource(
                  form.watch('url'),
                  features.name ? form.watch('suggestedName') : '',
                ) || 'Uncategorized'
              }
            />
          </div>
          <div className="divide-y rounded-lg border">
            <section aria-label="Organization" className="p-3">
              <Label className="flex cursor-pointer items-center gap-3">
                <input
                  type="checkbox"
                  checked={features.organization}
                  disabled={pending || added}
                  onChange={(e) =>
                    setFeatures({
                      ...features,
                      organization: e.target.checked,
                      queue: e.target.checked,
                    })
                  }
                />
                Organize download
                <span className="ml-auto text-xs font-normal text-muted-foreground">
                  Queue, priority, tags
                </span>
              </Label>
              {features.organization && (
                <div className="pt-3">
                  <TaskOptionsForm
                    value={options}
                    onChange={setOptions}
                    disabled={pending || added}
                    sections={{
                      queue: true,
                      organization: true,
                      credentials: false,
                      schedule: false,
                      category: false,
                    }}
                  />
                </div>
              )}
            </section>
            <section aria-label="Scheduling" className="p-3">
              <TaskOptionsForm
                value={options}
                onChange={setOptions}
                disabled={pending || added}
                sections={{
                  queue: false,
                  organization: false,
                  credentials: false,
                }}
              />
            </section>
            <details className="group p-3">
              <summary className="cursor-pointer text-sm font-medium">
                More options
                <span className="ml-2 text-xs font-normal text-muted-foreground">
                  Filename, verification, sign-in
                </span>
              </summary>
              <div className="mt-3 space-y-3 border-t pt-3">
                <div className="space-y-2">
                  {toggle('name', 'Custom filename')}
                  {features.name && (
                    <Input
                      aria-label="Filename"
                      {...form.register('suggestedName')}
                      placeholder="Name from the link"
                    />
                  )}
                </div>
                <div className="space-y-2">
                  {toggle('checksum', 'Verify SHA-256')}
                  {features.checksum && (
                    <>
                      <Input
                        aria-label="SHA-256"
                        {...form.register('expectedSha256')}
                        autoComplete="off"
                        spellCheck={false}
                        placeholder="64-character checksum"
                        aria-invalid={!!form.formState.errors.expectedSha256}
                      />
                      {form.formState.errors.expectedSha256 && (
                        <p role="alert" className="text-sm text-destructive">
                          {form.formState.errors.expectedSha256.message}
                        </p>
                      )}
                    </>
                  )}
                </div>
                <div className="space-y-2">
                  {toggle('credentials', 'Use saved credentials')}
                  {features.credentials && (
                    <TaskOptionsForm
                      value={options}
                      onChange={setOptions}
                      disabled={pending || added}
                      sections={{
                        queue: false,
                        organization: false,
                        credentials: true,
                        schedule: false,
                      }}
                    />
                  )}
                </div>
                <div className="space-y-2">
                  {toggle('acceleration', 'Multiple connections')}
                  {features.acceleration && (
                    <select
                      aria-label="Connections"
                      className={selectClass}
                      value={options.connections ?? 4}
                      onChange={(e) =>
                        setOptions({
                          ...options,
                          connections: Number(e.target.value),
                        })
                      }
                    >
                      {[2, 3, 4].map((n) => (
                        <option key={n} value={n}>
                          {n} connections
                        </option>
                      ))}
                    </select>
                  )}
                </div>
              </div>
            </details>
          </div>
        </fieldset>
        <QueueStartNotice
          queueId={features.queue ? options.queueId : 'default'}
          startNow={form.watch('startNow')}
          disabled={pending || added}
        />
        {added && (
          <p role="status" className="text-sm">
            Download added. You can close this window.
          </p>
        )}
        {form.formState.errors.root && (
          <p role="alert" className="text-sm text-destructive">
            {form.formState.errors.root.message}
          </p>
        )}
      </div>
      <footer className="flex shrink-0 items-center justify-between gap-3 border-t bg-background px-5 py-3">
        <Label className="flex items-center gap-2 text-sm normal-case">
          <input
            type="checkbox"
            {...form.register('startNow')}
            disabled={pending || added}
          />
          Start automatically
        </Label>
        <div className="flex gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={pending}
            onClick={() => {
              void Promise.resolve(onDone()).catch(() =>
                form.setError('root', {
                  message: 'Could not close this window.',
                }),
              )
            }}
          >
            Close
          </Button>
          <Button
            type="submit"
            disabled={
              pending ||
              added ||
              settings.isPending ||
              destinations.isPending ||
              settings.isError ||
              destinations.isError
            }
          >
            {pending ? 'Adding…' : 'Add download'}
          </Button>
        </div>
      </footer>
    </form>
  )
}
