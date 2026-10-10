import { createFileRoute } from '@tanstack/react-router'
import { downloadsListOptions } from '@/features/downloads/api/downloads'
import { TaskDetailsSheet } from '@/features/downloads/components/task-details-sheet'
import { downloadsSearchSchema } from '@/features/downloads/lib/task-query'
import { DownloadsPage } from '@/features/downloads/pages/downloads-page'

export const Route = createFileRoute('/downloads/$downloadId')({
  validateSearch: search => downloadsSearchSchema.parse(search),
  loader: ({ context }) => {
    void context.queryClient.prefetchQuery(downloadsListOptions(context.queryClient))
  },
  component: DownloadDetailsRoute,
})

function DownloadDetailsRoute() {
  const search = Route.useSearch()
  const { downloadId } = Route.useParams()
  return <DownloadsPage search={search} detailsSlot={<TaskDetailsSheet downloadId={downloadId} />} />
}
