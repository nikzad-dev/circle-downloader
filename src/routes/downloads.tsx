import { createFileRoute } from '@tanstack/react-router'
import { downloadsListOptions } from '@/features/downloads/api/downloads'
import { DownloadsPage } from '@/features/downloads/pages/downloads-page'

export const Route = createFileRoute('/downloads')({
  loader: ({ context }) => { void context.queryClient.prefetchQuery(downloadsListOptions(context.queryClient)) },
  component: DownloadsPage,
})
