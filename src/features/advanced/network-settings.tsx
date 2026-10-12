import type { NetworkSettings } from './api'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { advanced, advancedKeys, useGrants } from './api'
import { selectClass } from './task-options-form'

export function NetworkPreferences() {
  const query = useQuery({
    queryKey: advancedKeys.network,
    queryFn: advanced.network,
    retry: false,
  })
  return (
    <>
      <Card size="sm">
        <CardHeader>
          <CardTitle>Network and rate limits</CardTitle>
          <CardDescription>
            Global limits cover every download and segment. Proxy and timeout
            changes require paused transfers.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {query.isPending && <p>Loading network settings…</p>}
          {query.isError && (
            <Button
              size="sm"
              variant="outline"
              onClick={() => void query.refetch()}
            >
              Retry network settings
            </Button>
          )}
          {query.data && <NetworkForm initial={query.data} />}
        </CardContent>
      </Card>
      <CredentialsForm />
    </>
  )
}
function NetworkForm({ initial }: { initial: NetworkSettings }) {
  const [value, setValue] = useState(initial)
  const [message, setMessage] = useState('')
  const [pending, setPending] = useState(false)
  const grants = useGrants()
  const client = useQueryClient()
  const save = async () => {
    setPending(true)
    setMessage('')
    try {
      setValue(await advanced.saveNetwork(value))
      await client.invalidateQueries({ queryKey: advancedKeys.network })
      setMessage('Network settings saved.')
    } catch (e) {
      setMessage(
        e instanceof Error ? e.message : 'Could not save network settings.',
      )
    } finally {
      setPending(false)
    }
  }
  return (
    <form
      className="space-y-3"
      onSubmit={(e) => {
        e.preventDefault()
        void save()
      }}
    >
      <fieldset disabled={pending} className="space-y-3">
        <Label className="block space-y-1">
          Global rate (KiB/s; 0 unlimited)
          <Input
            aria-label="Global rate KiB/s"
            type="number"
            min={0}
            max={976562}
            value={value.bytesPerSecond / 1024}
            onChange={(e) =>
              setValue({
                ...value,
                bytesPerSecond: Math.round(Number(e.target.value) * 1024),
              })
            }
          />
        </Label>
        <Label className="block space-y-1">
          Proxy origin
          <Input
            aria-label="Proxy origin"
            placeholder="http://proxy.example.com:8080"
            value={value.proxyUrl}
            onChange={(e) =>
              setValue({
                ...value,
                proxyUrl: e.target.value,
                proxyCredentialId: null,
              })
            }
            autoComplete="off"
          />
        </Label>
        <Label className="block space-y-1">
          Proxy authorization
          <select
            aria-label="Proxy authorization"
            className={selectClass}
            value={value.proxyCredentialId ?? ''}
            onChange={(e) =>
              setValue({ ...value, proxyCredentialId: e.target.value || null })
            }
          >
            <option value="">No credentials</option>
            {grants.data
              ?.filter((g) => g.kind === 'proxy')
              .map((g) => (
                <option key={g.id} value={g.id}>
                  {g.scope}
                </option>
              ))}
          </select>
        </Label>
        <Label className="block space-y-1">
          Connection timeout (seconds)
          <Input
            aria-label="Connection timeout seconds"
            type="number"
            min={1}
            max={120}
            value={value.connectTimeoutSecs}
            onChange={(e) =>
              setValue({ ...value, connectTimeoutSecs: Number(e.target.value) })
            }
          />
        </Label>
        <p className="text-xs text-muted-foreground">
          Only explicitly configured HTTP/HTTPS proxies are used. System proxy
          environment variables are ignored. The proxy is trusted to resolve
          destinations; TLS certificate validation remains enabled.
        </p>
      </fieldset>
      {message !== '' && (
        <p role="status" className="text-xs">
          {message}
        </p>
      )}
      <Button size="sm" type="submit" disabled={pending}>
        {pending ? 'Saving…' : 'Save network settings'}
      </Button>
    </form>
  )
}
function CredentialsForm() {
  const [kind, setKind] = useState<'origin' | 'proxy'>('origin')
  const [scope, setScope] = useState('')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [message, setMessage] = useState('')
  const [pending, setPending] = useState(false)
  const grants = useGrants()
  const client = useQueryClient()
  const save = async () => {
    setPending(true)
    setMessage('')
    try {
      await advanced.saveCredential({ kind, scope, username, password })
      setUsername('')
      setScope('')
      setMessage('Authorization saved in the OS credential store.')
      await client.invalidateQueries({ queryKey: advancedKeys.grants })
    } catch (e) {
      setMessage(
        e instanceof Error ? e.message : 'Could not save authorization.',
      )
    } finally {
      setPassword('')
      setPending(false)
    }
  }
  const revoke = async (id: string) => {
    setPending(true)
    setMessage('')
    try {
      await advanced.revokeCredential(id)
      await client.invalidateQueries({ queryKey: advancedKeys.grants })
      setMessage('Authorization revoked.')
    } catch (e) {
      setMessage(
        e instanceof Error ? e.message : 'Could not revoke authorization.',
      )
    } finally {
      setPending(false)
    }
  }
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Credential authorization</CardTitle>
        <CardDescription>
          Passwords stay in the OS credential store. Grant access to one exact
          origin, then select the grant for a download or proxy. Downloads with
          origin credentials reject redirects to other origins.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <form
          className="space-y-3"
          onSubmit={(e) => {
            e.preventDefault()
            void save()
          }}
        >
          <fieldset disabled={pending} className="space-y-3">
            <Label className="block space-y-1">
              Authorization type
              <select
                aria-label="Authorization type"
                className={selectClass}
                value={kind}
                onChange={(e) => setKind(e.target.value as 'origin' | 'proxy')}
              >
                <option value="origin">Download origin (HTTPS)</option>
                <option value="proxy">Proxy</option>
              </select>
            </Label>
            <Label className="block space-y-1">
              Exact origin
              <Input
                aria-label="Credential origin"
                type="url"
                placeholder="https://example.com"
                value={scope}
                onChange={(e) => setScope(e.target.value)}
                autoComplete="off"
                required
              />
            </Label>
            <Label className="block space-y-1">
              Username
              <Input
                aria-label="Credential username"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                autoComplete="off"
              />
            </Label>
            <Label className="block space-y-1">
              Password
              <Input
                aria-label="Credential password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="new-password"
                required
              />
            </Label>
          </fieldset>
          <Button type="submit" size="sm" disabled={pending}>
            Authorize and save credentials
          </Button>
        </form>
        {message !== '' && (
          <p role="status" className="text-xs">
            {message}
          </p>
        )}
        {grants.isError && (
          <Button
            variant="outline"
            size="sm"
            onClick={() => void grants.refetch()}
          >
            Retry authorizations
          </Button>
        )}
        <ul className="space-y-2">
          {grants.data?.map((g) => (
            <li
              key={g.id}
              className="flex flex-wrap items-center justify-between gap-2 text-xs"
            >
              <span>
                {g.kind} · {g.scope}
              </span>
              <Button
                variant="outline"
                size="sm"
                disabled={pending}
                onClick={() => void revoke(g.id)}
              >
                Revoke
              </Button>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  )
}
