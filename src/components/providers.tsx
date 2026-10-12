import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createRouter, RouterProvider } from '@tanstack/react-router'
import { invoke } from '@tauri-apps/api/core'
import { lazy, Suspense, useEffect } from 'react'
import { ThemeProvider } from '@/components/theme/theme-provider'
import { Toaster } from '@/components/ui/toast'
import { DownloadProgressProvider } from '@/features/downloads/hooks/download-progress'
import { routeTree } from '../routeTree.gen'

const AddDownloadWindow = lazy(async () => {
  const module =
    await import('@/features/downloads/components/add-download-window')
  return { default: module.DownloadWindowHost }
})
const windowParams = new URLSearchParams(window.location.search)
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 30_000, retry: 1, refetchOnWindowFocus: false },
    mutations: { retry: false },
  },
})
const router = createRouter({
  routeTree,
  context: { queryClient },
  defaultPreload: 'intent',
  scrollRestoration: true,
})
declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
export function AppProviders() {
  useEffect(() => {
    if (windowParams.get('downloadWindow') === '1') return
    // Prepare after the main UI paints, outside the Add button's critical path.
    const timer = window.setTimeout(() => {
      void invoke('prepare_download_window').catch(() => {})
    }, 300)
    return () => window.clearTimeout(timer)
  }, [])
  return (
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        {windowParams.get('downloadWindow') === '1' ? (
          <Suspense fallback={null}>
            <AddDownloadWindow />
          </Suspense>
        ) : (
          <DownloadProgressProvider>
            <RouterProvider router={router} />
          </DownloadProgressProvider>
        )}
        <Toaster />
      </QueryClientProvider>
    </ThemeProvider>
  )
}
