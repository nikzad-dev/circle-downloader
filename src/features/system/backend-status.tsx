import { useQuery } from '@tanstack/react-query'
import { orpc } from '@/lib/orpc/client'

/** Optional component you can mount on the dashboard or settings page. */
export function BackendStatus() {
  const health = useQuery({
    ...orpc.system.health.queryOptions(),
    staleTime: 60_000,
    retry: false,
  })

  if (health.isPending) return <span>Checking Rust backend…</span>
  if (health.isError) return <span role="alert">Rust backend unavailable</span>
  return <span>Rust backend connected ({health.data})</span>
}
