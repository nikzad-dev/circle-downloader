import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { orpc } from '@/lib/orpc/client'

export const Route = createFileRoute('/health_check')({
  loader: async ({ context }) =>
    context.queryClient.ensureQueryData(orpc.system.health.queryOptions()),
  component: HealthCheckPage,
})

function HealthCheckPage() {
  const { data, isPending, error, refetch, isFetching } = useQuery({
    ...orpc.system.health.queryOptions(),
    staleTime: 60_000,
    retry: false,
  })

  return (
    <section className="space-y-4">
      <div className="space-y-2">
        <h1 className="font-heading text-2xl font-semibold tracking-tight">
          Health check
        </h1>
        <p className="text-muted-foreground">
          Verifies the Tauri Rust backend is reachable.
        </p>
      </div>

      {isPending
        ? (
            <p>Checking Rust backend…</p>
          )
        : error
          ? (
              <p role="alert">
                Rust backend unavailable:
                {' '}
                {error.message}
              </p>
            )
          : (
              <p>
                Status:
                {' '}
                <span className="font-medium text-foreground">{data}</span>
              </p>
            )}

      <button
        type="button"
        className="text-sm text-muted-foreground underline underline-offset-4 hover:text-foreground"
        onClick={() => void refetch()}
        disabled={isFetching}
      >
        {isFetching ? 'Checking…' : 'Re-check'}
      </button>
    </section>
  )
}
