import { Button } from '@/components/ui/button'
import { useDownloads } from '../api/downloads'
import { DownloadsEmpty, DownloadsError, DownloadsLoading } from '../components/downloads-state'
import { DownloadsTable } from '../components/downloads-table'
import { useProgressConnection } from '../hooks/progress-context'

export function DownloadsPage() {
  const query = useDownloads()
  const connection = useProgressConnection()
  return (
    <section className="space-y-6">
      <div className="flex items-end justify-between gap-4">
        <div>
          <p className="mb-2 text-xs font-semibold tracking-widest text-muted-foreground uppercase">Your library</p>
          <h1 className="font-heading text-3xl font-semibold tracking-tight">Downloads</h1>
          <p className="mt-2 text-sm text-muted-foreground">Every file. Every step. All in one place.</p>
        </div>
        <span className="shrink-0 pb-1 text-xs text-muted-foreground tabular-nums">{query.data ? `${query.data.length} ${query.data.length === 1 ? 'file' : 'files'}` : '—'}</span>
      </div>
      {connection.unavailable && (
        <div role="status" className="flex flex-wrap items-center justify-between gap-3 border bg-muted/40 p-3 text-sm">
          <span>Live updates unavailable. Refresh to check progress.</span>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              connection.reconnect()
              void query.refetch()
            }}
          >
            Reconnect
          </Button>
        </div>
      )}
      {query.isError && <DownloadsError retry={() => void query.refetch()} pending={query.isFetching} background={query.data !== undefined} />}
      {query.isPending ? <DownloadsLoading /> : query.data && (query.data.length ? <div className="border bg-card"><DownloadsTable tasks={query.data} /></div> : <DownloadsEmpty />)}
    </section>
  )
}
