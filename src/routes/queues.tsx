import { createFileRoute } from '@tanstack/react-router'
import { QueuesPage } from '@/features/advanced/queues-page'

export const Route = createFileRoute('/queues')({ component: QueuesPage })
