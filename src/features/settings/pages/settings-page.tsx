import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { open } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'
import { ThemeSelector } from '@/components/theme/theme-selector'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { toast } from '@/components/ui/toast'
import { NetworkPreferences } from '@/features/advanced/network-settings'
import { downloadErrorMessage } from '@/features/downloads/lib/errors'
import { orpc } from '@/lib/orpc/client'
import { strings } from '@/lib/ui-strings'

export function SettingsPage() {
  const client = useQueryClient()
  const settings = useQuery(orpc.settings.get.queryOptions())
  const destinations = useQuery(orpc.destinations.list.queryOptions())
  const [concurrency, setConcurrency] = useState<number | null>(null)
  const [connections, setConnections] = useState<number | null>(null)

  const update = useMutation({
    mutationFn: async (
      input: Parameters<typeof orpc.settings.update.call>[0],
    ) => orpc.settings.update.call(input),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: orpc.settings.get.key() })
      toast.add({ title: 'Settings saved', type: 'success' })
    },
    onError: (error) => {
      toast.add({
        title: 'Could not save settings',
        description: downloadErrorMessage(error),
        type: 'error',
      })
    },
  })

  const grant = useMutation({
    mutationFn: async () => {
      const selected = await open({ directory: true, multiple: false })
      if (selected === null) return null
      return orpc.destinations.grant.call({ path: selected })
    },
    onSuccess: async (result) => {
      if (!result) return
      await client.invalidateQueries({ queryKey: orpc.destinations.list.key() })
      toast.add({ title: 'Folder approved', type: 'success' })
    },
    onError: (error) => {
      toast.add({
        title: 'Could not add folder',
        description: downloadErrorMessage(error),
        type: 'error',
      })
    },
  })

  const remove = useMutation({
    mutationFn: async (id: string) => orpc.destinations.remove.call({ id }),
    onSuccess: async () => {
      await Promise.all([
        client.invalidateQueries({ queryKey: orpc.destinations.list.key() }),
        client.invalidateQueries({ queryKey: orpc.settings.get.key() }),
      ])
      toast.add({ title: 'Folder removed', type: 'success' })
    },
    onError: (error) => {
      toast.add({
        title: 'Could not remove folder',
        description: downloadErrorMessage(error),
        type: 'error',
      })
    },
  })

  const effectiveConcurrency =
    concurrency ?? settings.data?.maxConcurrentDownloads ?? 3
  const effectiveConnections =
    connections ?? settings.data?.maxConnectionsPerDownload ?? 1

  return (
    <section className="grid items-start gap-3 lg:grid-cols-2 xl:grid-cols-3">
      <h1 className="sr-only">{strings.settings}</h1>

      <Card size="sm">
        <CardHeader>
          <CardTitle>General</CardTitle>
          <CardDescription>Appearance and window behavior.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <ThemeSelector />
          <p className="text-sm leading-relaxed text-muted-foreground">
            {strings.quitPolicy}
          </p>
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle>Downloads</CardTitle>
          <CardDescription>Transfer preferences.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-5">
          {(settings.isError || destinations.isError) && (
            <div role="alert" className="space-y-2 text-sm text-destructive">
              <p>Could not load download settings.</p>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  void settings.refetch()
                  void destinations.refetch()
                }}
              >
                Retry
              </Button>
            </div>
          )}
          {settings.data && (
            <>
              <Label className="flex items-center gap-3 text-sm">
                <Checkbox
                  checked={settings.data.autoStart}
                  disabled={update.isPending}
                  onCheckedChange={(checked) =>
                    update.mutate({ autoStart: !!checked })
                  }
                />
                {strings.autoStart}
              </Label>
              <div className="space-y-2">
                <Label htmlFor="concurrency">{strings.concurrency}</Label>
                <div className="flex items-center gap-3">
                  <Input
                    id="concurrency"
                    type="number"
                    min={1}
                    max={32}
                    className="max-w-28"
                    value={effectiveConcurrency}
                    disabled={update.isPending}
                    onChange={(event) =>
                      setConcurrency(Number(event.target.value))
                    }
                    onBlur={() => {
                      const value = Math.min(
                        32,
                        Math.max(1, effectiveConcurrency),
                      )
                      setConcurrency(null)
                      if (value !== settings.data.maxConcurrentDownloads)
                        update.mutate({ maxConcurrentDownloads: value })
                    }}
                  />
                </div>
              </div>
              <div className="space-y-2">
                <Label htmlFor="connections">
                  {strings.connectionsPerDownload}
                </Label>
                <div className="flex items-center gap-3">
                  <Input
                    id="connections"
                    type="number"
                    min={1}
                    max={4}
                    className="max-w-28"
                    value={effectiveConnections}
                    disabled={update.isPending}
                    onChange={(event) =>
                      setConnections(Number(event.target.value))
                    }
                    onBlur={() => {
                      const value = Number.isFinite(effectiveConnections)
                        ? Math.min(
                            4,
                            Math.max(1, Math.trunc(effectiveConnections)),
                          )
                        : 1
                      setConnections(null)
                      if (value !== settings.data.maxConnectionsPerDownload)
                        update.mutate({ maxConnectionsPerDownload: value })
                    }}
                  />
                </div>
                <p className="text-xs text-muted-foreground">
                  {strings.connectionsPerDownloadHint}
                </p>
              </div>
              <div className="space-y-2">
                <Label htmlFor="default-destination">Default destination</Label>
                <select
                  id="default-destination"
                  className="h-10 w-full max-w-md border-b border-input bg-transparent text-sm outline-none"
                  value={settings.data.defaultDestinationId}
                  disabled={update.isPending || !destinations.data}
                  onChange={(event) =>
                    update.mutate({ defaultDestinationId: event.target.value })
                  }
                >
                  {(destinations.data ?? []).map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.label}
                    </option>
                  ))}
                </select>
                <p className="text-xs text-muted-foreground">
                  {strings.collisionPolicy}
                </p>
              </div>
              <ul className="space-y-1 text-sm text-muted-foreground">
                <li>{strings.retriesReadonly}</li>
                <li>{strings.perHostReadonly}</li>
              </ul>
            </>
          )}
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle>Approved folders</CardTitle>
          <CardDescription>
            Choose where downloads can be saved.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Button
            size="sm"
            disabled={grant.isPending}
            onClick={() => void grant.mutateAsync()}
          >
            {strings.addFolder}
          </Button>
          <ul className="space-y-3">
            {(destinations.data ?? []).map((item) => (
              <li
                key={item.id}
                className="flex flex-wrap items-start justify-between gap-3 border-b pb-3 text-sm"
              >
                <div className="min-w-0">
                  <p className="font-medium">{item.label}</p>
                  <p className="break-all text-xs text-muted-foreground">
                    {item.displayPath}
                  </p>
                </div>
                {!item.isDefault && (
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={remove.isPending}
                    onClick={() => remove.mutate(item.id)}
                  >
                    Remove
                  </Button>
                )}
              </li>
            ))}
          </ul>
        </CardContent>
      </Card>
      <NetworkPreferences />
    </section>
  )
}
