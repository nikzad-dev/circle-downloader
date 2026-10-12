import { Download } from 'lucide-react'
import { Card, CardContent } from '@/components/ui/card'
import { useDownloads } from '../api/downloads'
import {
  DownloadsEmpty,
  DownloadsError,
  DownloadsLoading,
} from '../components/downloads-state'
import { DownloadsTable } from '../components/downloads-table'
import { activeStatuses } from '../lib/format'

export function HomeOverviewPage() {
  const query = useDownloads()
  const tasks = query.data ?? []
  const summaries = [
    {
      label: 'Active transfers',
      value: tasks.filter((task) => activeStatuses.has(task.status)).length,
    },
    {
      label: 'Completed',
      value: tasks.filter((task) => task.status === 'completed').length,
    },
    {
      label: 'Needs attention',
      value: tasks.filter((task) => task.status === 'failed').length,
    },
  ]
  const recent = [...tasks]
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
    .slice(0, 5)
  const failures = [...tasks]
    .filter((task) => task.status === 'failed')
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
    .slice(0, 3)
  return (
    <section className="space-y-3">
      <h1 className="sr-only">Overview</h1>
      <div className="grid grid-cols-3 gap-3">
        {summaries.map((summary) => (
          <Card key={summary.label} size="sm">
            <CardContent>
              <p className="text-xs leading-relaxed text-muted-foreground">
                {summary.label}
              </p>
              <p className="mt-1 font-heading text-lg font-semibold tabular-nums">
                {query.data ? summary.value : '—'}
              </p>
            </CardContent>
          </Card>
        ))}
      </div>
      {query.isError && (
        <DownloadsError
          retry={() => void query.refetch()}
          pending={query.isFetching}
          background={query.data !== undefined}
        />
      )}
      {query.isPending ? (
        <DownloadsLoading />
      ) : (
        query.data &&
        (tasks.length ? (
          <>
            <div>
              <div className="mb-2 flex items-center gap-2">
                <Download
                  className="size-4 text-muted-foreground"
                  aria-hidden="true"
                />
                <h2 className="font-heading text-sm font-semibold">
                  Recent downloads
                </h2>
              </div>
              <div className="border bg-card">
                <DownloadsTable tasks={recent} showSelection={false} />
              </div>
            </div>
            {failures.length > 0 && (
              <div>
                <h2 className="mb-2 font-heading text-sm font-semibold">
                  Recent failures
                </h2>
                <div className="border bg-card">
                  <DownloadsTable tasks={failures} showSelection={false} />
                </div>
              </div>
            )}
          </>
        ) : (
          <DownloadsEmpty />
        ))
      )}
    </section>
  )
}
