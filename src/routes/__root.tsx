import type { QueryClient } from '@tanstack/react-query'
import { createRootRouteWithContext } from '@tanstack/react-router'
import { AppShell } from '@/components/app-shell'
import { Button } from '@/components/ui/button'

export const Route = createRootRouteWithContext<{ queryClient: QueryClient }>()(
  {
    component: AppShell,
    errorComponent: ({ reset }) => (
      <div role="alert" className="space-y-4 p-6">
        <h1 className="font-heading text-xl font-semibold">
          This page could not be opened
        </h1>
        <Button onClick={reset}>Try again</Button>
      </div>
    ),
  },
)
