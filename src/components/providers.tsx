import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createRouter, RouterProvider } from '@tanstack/react-router'
import { ThemeProvider } from '@/components/theme/theme-provider'
import { Toaster } from '@/components/ui/toast'
import { DownloadProgressProvider } from '@/features/downloads/hooks/download-progress'
import { routeTree } from '../routeTree.gen'

const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: 30_000, retry: 1, refetchOnWindowFocus: false }, mutations: { retry: false } } })
const router = createRouter({ routeTree, context: { queryClient }, defaultPreload: 'intent', scrollRestoration: true })
declare module '@tanstack/react-router' { interface Register { router: typeof router } }
export function AppProviders() {
  return (
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <DownloadProgressProvider>
          <RouterProvider router={router} />
          <Toaster />
        </DownloadProgressProvider>
      </QueryClientProvider>
    </ThemeProvider>
  )
}
