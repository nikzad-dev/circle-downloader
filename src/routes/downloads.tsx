import { createFileRoute } from '@tanstack/react-router'
import { downloadsListOptions } from '@/features/downloads/api/downloads'
import { downloadsSearchSchema } from '@/features/downloads/lib/task-query'
import { DownloadsPage } from '@/features/downloads/pages/downloads-page'

export const Route = createFileRoute('/downloads')({
  validateSearch: search => downloadsSearchSchema.parse(search),
  loader: ({ context }) => {
    void context.queryClient.prefetchQuery(downloadsListOptions(context.queryClient))
  },
  component: DownloadsRoute,
})

function DownloadsRoute() {
  const search = Route.useSearch()
  return <DownloadsPage search={search} />
}
