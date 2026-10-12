import { createFileRoute } from '@tanstack/react-router'
import { HomeOverviewPage } from '@/features/downloads/pages/home-overview-page'

export const Route = createFileRoute('/overview')({
  component: HomeOverviewPage,
})
