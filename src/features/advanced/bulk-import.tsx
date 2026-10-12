import { useQueryClient } from '@tanstack/react-query'
import { ListPlus } from 'lucide-react'
import { useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { downloadsKey } from '@/features/downloads/api/downloads'
import {
  closeDownloadWindow,
  openDownloadWindow,
} from '@/features/downloads/components/download-window-api'
import { openProgressWindow } from '@/features/downloads/components/progress-window-api'
import { advanced, advancedKeys, emptyTaskOptions } from './api'
import { QueueStartNotice } from './queue-start-notice'
import { CategorySelect, TaskOptionsForm } from './task-options-form'

export function BulkImportButton() {
  return (
    <Button
      variant="outline"
      size="sm"
      aria-label="Import URLs"
      onClick={() => void openDownloadWindow('', true)}
    >
      <ListPlus className="size-4" aria-hidden="true" />
      <span className="hidden lg:inline">Import URLs</span>
    </Button>
  )
}
export function BulkImportForm({
  onBusyChange,
  initialUrls = [],
}: {
  onBusyChange?: (busy: boolean) => void
  initialUrls?: string[]
}) {
  const client = useQueryClient()
  const [text, setText] = useState(initialUrls.join('\n'))
  const [options, setOptions] = useState(emptyTaskOptions)
  const [features, setFeatures] = useState({
    queue: false,
    organization: false,
    credentials: false,
    acceleration: false,
  })
  const [startNow, setStartNow] = useState(true)
  const [pending, setPending] = useState(false)
  const [message, setMessage] = useState('')
  const batchRef = useRef('')
  const urls = [
    ...new Set(
      text
        .split(/\r?\n/)
        .map((s) => s.trim())
        .filter(Boolean),
    ),
  ]
  const submit = async () => {
    setPending(true)
    onBusyChange?.(true)
    setMessage('')
    if (!batchRef.current) batchRef.current = crypto.randomUUID()
    try {
      const result = await advanced.import({
        urls,
        options: {
          ...options,
          queueId: features.queue ? options.queueId : 'default',
          priority: features.queue ? options.priority : 0,
          category: options.category,
          tags: features.organization ? options.tags : [],
          credentialId: features.credentials ? options.credentialId : null,
          connections: features.acceleration ? 4 : 1,
        },
        startNow,
        batchId: batchRef.current,
      })
      if (startNow && result.created.length > 0) void openProgressWindow()
      setMessage(
        `Imported ${result.created.length} downloads.${result.errors.length ? ` ${result.errors.join(' ')}` : ''}`,
      )
      await Promise.all([
        client.invalidateQueries({ queryKey: downloadsKey }),
        client.invalidateQueries({ queryKey: advancedKeys.tasks }),
        client.invalidateQueries({ queryKey: advancedKeys.queues }),
      ])
      if (result.errors.length === 0) {
        setText('')
        batchRef.current = ''
      }
    } catch (e) {
      setMessage(e instanceof Error ? e.message : 'Import failed.')
    } finally {
      setPending(false)
      onBusyChange?.(false)
    }
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 space-y-4 overflow-auto p-5">
        <div className="space-y-2">
          <Label htmlFor="bulk-urls">Download URLs</Label>
          <textarea
            id="bulk-urls"
            className="min-h-36 w-full rounded-md border bg-background p-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            value={text}
            disabled={pending}
            onChange={(e) => {
              setText(e.target.value)
              batchRef.current = ''
            }}
            autoComplete="off"
            spellCheck={false}
            placeholder="One URL per line"
          />
          <p className="text-xs text-muted-foreground">
            Up to 200 links. Duplicate lines are skipped.
          </p>
        </div>
        <div className="space-y-2">
          <Label htmlFor="bulk-category">Category</Label>
          <CategorySelect
            id="bulk-category"
            value={options.category}
            onChange={(category) => setOptions({ ...options, category })}
            automaticCategory="each file separately"
          />
        </div>
        <div className="divide-y rounded-lg border">
          <section className="p-3">
            <Label className="flex cursor-pointer items-center gap-3">
              <input
                type="checkbox"
                checked={features.organization}
                disabled={pending}
                onChange={(e) =>
                  setFeatures({
                    ...features,
                    organization: e.target.checked,
                    queue: e.target.checked,
                  })
                }
              />
              Organize downloads
              <span className="ml-auto text-xs font-normal text-muted-foreground">
                Queue, priority, tags
              </span>
            </Label>
            {features.organization && (
              <div className="pt-3">
                <TaskOptionsForm
                  value={options}
                  onChange={setOptions}
                  disabled={pending}
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
          <section className="p-3">
            <TaskOptionsForm
              value={options}
              onChange={setOptions}
              disabled={pending}
              sections={{
                queue: false,
                organization: false,
                credentials: false,
              }}
            />
          </section>
          <details className="p-3">
            <summary className="cursor-pointer text-sm font-medium">
              More options
            </summary>
            <div className="mt-3 space-y-3 border-t pt-3">
              <Label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={features.credentials}
                  disabled={pending}
                  onChange={(e) =>
                    setFeatures({ ...features, credentials: e.target.checked })
                  }
                />
                Use saved credentials
              </Label>
              {features.credentials && (
                <TaskOptionsForm
                  value={options}
                  onChange={setOptions}
                  disabled={pending}
                  sections={{
                    queue: false,
                    organization: false,
                    credentials: true,
                    schedule: false,
                  }}
                />
              )}
              <Label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={features.acceleration}
                  disabled={pending}
                  onChange={(e) =>
                    setFeatures({ ...features, acceleration: e.target.checked })
                  }
                />
                Multiple connections
              </Label>
            </div>
          </details>
        </div>
        <QueueStartNotice
          queueId={features.queue ? options.queueId : 'default'}
          startNow={startNow}
          disabled={pending}
        />
        {message !== '' && (
          <p role="status" className="text-sm">
            {message}
          </p>
        )}
      </div>
      <footer className="flex shrink-0 items-center justify-between gap-3 border-t px-5 py-3">
        <Label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={startNow}
            disabled={pending}
            onChange={(e) => setStartNow(e.target.checked)}
          />
          Queue after import
        </Label>
        <div className="flex gap-2">
          <Button
            variant="outline"
            disabled={pending}
            onClick={() =>
              void closeDownloadWindow().catch(() =>
                setMessage('Could not close the download window.'),
              )
            }
          >
            Close
          </Button>
          <Button
            disabled={pending || urls.length === 0 || urls.length > 200}
            onClick={() => void submit()}
          >
            {pending ? 'Importing…' : `Import ${urls.length} URLs`}
          </Button>
        </div>
      </footer>
    </div>
  )
}
