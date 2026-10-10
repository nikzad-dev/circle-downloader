import { createFileRoute } from '@tanstack/react-router'
import { downloadsListOptions } from '@/features/downloads/api/downloads'
import { HomeOverviewPage } from '@/features/downloads/pages/home-overview-page'

export const Route = createFileRoute('/')({
  loader: ({ context }) => { void context.queryClient.prefetchQuery(downloadsListOptions(context.queryClient)) },
  component: HomeOverviewPage,
})
