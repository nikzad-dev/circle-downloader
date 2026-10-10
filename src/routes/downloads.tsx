import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'

import { orpc } from '@/lib/orpc/client'

function downloadsListQueryOptions() {
  return orpc.downloads.list.queryOptions({
    staleTime: 5_000,
  })
}

export const Route = createFileRoute('/downloads')({
  loader: async ({ context }) =>
    context.queryClient.query(
      downloadsListQueryOptions(),
    ),
  component: DownloadsPage,
})

function DownloadsPage() {
  const {
    data: downloads = [],
    isPending,
    error,
  } = useQuery(downloadsListQueryOptions())

  if (isPending) {
    return <p>Loading downloads…</p>
  }

  if (error) {
    return (
      <p role="alert">
        Unable to load downloads:
        {' '}
        {error.message}
      </p>
    )
  }

  return (
    <section className="space-y-4">
      <h1 className="text-2xl font-semibold">
        Downloads
      </h1>

      {downloads.length === 0 && (
        <p className="text-muted-foreground">
          No downloads yet.
        </p>
      )}

      {downloads.length > 0 && (
        <ul className="space-y-2">
          {downloads.map(download => (
            <li key={download.id}>
              {download.fileName}
              {' '}
              —
              {' '}
              {download.status}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
