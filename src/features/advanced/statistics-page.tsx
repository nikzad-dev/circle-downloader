import { useQuery } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { RollingNumber } from '@/components/ui/rolling-number'
import { formatBytes } from '@/features/downloads/lib/format'
import { advanced, advancedKeys } from './api'

export function StatisticsPage() {
  const query = useQuery({
    queryKey: advancedKeys.statistics,
    queryFn: advanced.statistics,
    retry: false,
    refetchInterval: 10_000,
  })
  const data = query.data
  return (
    <section aria-label="Transfer statistics" className="space-y-4">
      <h1 className="sr-only">Statistics</h1>
      <p className="text-sm text-muted-foreground">
        Recorded transfer attempts, including retries and interrupted work.
        Totals remain after a download is removed. The daily breakdown uses UTC.
      </p>
      {query.isPending && <p>Loading recorded measurements…</p>}
      {query.isError && (
        <Button variant="outline" onClick={() => void query.refetch()}>
          Retry statistics
        </Button>
      )}
      {data && (
        <>
          <div className="grid gap-3 sm:grid-cols-4">
            <Card size="sm">
              <CardContent>
                <p className="text-xs text-muted-foreground">Received bytes</p>
                <p className="mt-1 text-lg font-semibold tabular-nums">
                  {formatBytes(data.transferredBytes)}
                </p>
              </CardContent>
            </Card>
            <Card size="sm">
              <CardContent>
                <p className="text-xs text-muted-foreground">
                  Transfer worker time
                </p>
                <p className="mt-1 text-lg font-semibold tabular-nums">
                  <RollingNumber
                    value={Math.round(data.transferMillis / 1000)}
                  />{' '}
                  s
                </p>
              </CardContent>
            </Card>
            <Card size="sm">
              <CardContent>
                <p className="text-xs text-muted-foreground">
                  Completed attempts
                </p>
                <p className="mt-1 text-lg font-semibold tabular-nums">
                  <RollingNumber value={data.completedAttempts} />
                </p>
              </CardContent>
            </Card>
            <Card size="sm">
              <CardContent>
                <p className="text-xs text-muted-foreground">Failed attempts</p>
                <p className="mt-1 text-lg font-semibold tabular-nums">
                  <RollingNumber value={data.failedAttempts} />
                </p>
              </CardContent>
            </Card>
          </div>
          {data.days.length === 0 ? (
            <p className="rounded-md border p-4 text-sm text-muted-foreground">
              No recorded transfer attempts yet. Measurements appear after
              downloads finish or stop.
            </p>
          ) : (
            <table className="w-full text-sm">
              <caption className="pb-3 text-left font-medium">
                Recorded daily measurements (latest 30 days with activity)
              </caption>
              <thead>
                <tr className="border-b text-left text-xs text-muted-foreground">
                  <th className="py-2">Day (UTC)</th>
                  <th>Received bytes</th>
                  <th>Worker time</th>
                </tr>
              </thead>
              <tbody>
                {data.days.map((day) => (
                  <tr key={day.day} className="border-b tabular-nums">
                    <td className="py-2">{day.day}</td>
                    <td>{formatBytes(day.transferredBytes)}</td>
                    <td>{(day.transferMillis / 1000).toFixed(1)} s</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </>
      )}
    </section>
  )
}
