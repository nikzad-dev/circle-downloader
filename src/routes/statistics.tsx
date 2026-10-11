import { createFileRoute } from '@tanstack/react-router'
import { StatisticsPage } from '@/features/advanced/statistics-page'

export const Route = createFileRoute('/statistics')({ component: StatisticsPage })
