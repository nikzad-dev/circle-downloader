import { Link } from '@tanstack/react-router'
import { ArrowUpRight, Download, Plus } from 'lucide-react'
import { Button, buttonVariants } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { useDownloads } from '../api/downloads'
import { useAddDownload } from '../components/add-download-context'
import { DownloadsEmpty, DownloadsError, DownloadsLoading } from '../components/downloads-state'
import { DownloadsTable } from '../components/downloads-table'
import { activeStatuses } from '../lib/format'

export function HomeOverviewPage() {
  const query = useDownloads()
  const add = useAddDownload()
  const tasks = query.data ?? []
  const summaries = [
    { label: 'Active transfers', value: tasks.filter(task => activeStatuses.has(task.status)).length },
    { label: 'Completed', value: tasks.filter(task => task.status === 'completed').length },
    { label: 'Needs attention', value: tasks.filter(task => task.status === 'failed').length },
  ]
  const recent = [...tasks].sort((a, b) => b.createdAt.localeCompare(a.createdAt)).slice(0, 5)
  const failures = [...tasks].filter(task => task.status === 'failed').sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)).slice(0, 3)
  return (
    <section className="space-y-8">
      <div className="border-b pb-8">
        <p className="mb-3 text-xs font-semibold tracking-widest text-muted-foreground uppercase">Circle / Overview</p>
        <h1 className="font-heading text-4xl font-semibold tracking-tight">A little more in control.</h1>
        <p className="mt-3 max-w-md text-sm leading-relaxed text-muted-foreground">Add a link, follow its progress, and pick up where you left off.</p>
        <div className="mt-6 flex gap-3">
          <Button onClick={add}>
            <Plus aria-hidden="true" />
            Add download
          </Button>
          <Link to="/downloads" className={buttonVariants({ variant: 'outline' })}>
            View downloads
            <ArrowUpRight aria-hidden="true" />
          </Link>
        </div>
      </div>
      <div className="grid grid-cols-3 gap-3">
        {summaries.map(summary => (
          <Card key={summary.label} size="sm">
            <CardContent>
              <p className="text-xs leading-relaxed text-muted-foreground">{summary.label}</p>
              <p className="mt-3 font-heading text-3xl font-semibold tabular-nums">{query.data ? summary.value : '—'}</p>
            </CardContent>
          </Card>
        ))}
      </div>
      {query.isError && <DownloadsError retry={() => void query.refetch()} pending={query.isFetching} background={query.data !== undefined} />}
      {query.isPending
        ? <DownloadsLoading />
        : query.data && (tasks.length
          ? (
              <>
                <div>
                  <div className="mb-4 flex items-center gap-2">
                    <Download className="size-4 text-muted-foreground" aria-hidden="true" />
                    <h2 className="font-heading text-lg font-semibold">Recent downloads</h2>
                  </div>
                  <div className="border bg-card"><DownloadsTable tasks={recent} showSelection={false} /></div>
                </div>
                {failures.length > 0 && (
                  <div>
                    <h2 className="mb-4 font-heading text-lg font-semibold">Recent failures</h2>
                    <div className="border bg-card"><DownloadsTable tasks={failures} showSelection={false} /></div>
                  </div>
                )}
              </>
            )
          : <DownloadsEmpty />)}
    </section>
  )
}
