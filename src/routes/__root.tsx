import type { QueryClient } from '@tanstack/react-query'
import { createRootRouteWithContext, Link, Outlet } from '@tanstack/react-router'

interface RouterContext {
  queryClient: QueryClient
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: RootLayout,
})

const navItems = [
  { to: '/', label: 'Home' },
  { to: '/downloads', label: 'Downloads' },
  { to: '/settings', label: 'Settings' },
  { to: '/health_check', label: 'Health' },
] as const

function RootLayout() {
  return (
    <div className="min-h-svh bg-background text-foreground">
      <header className="border-b border-border">
        <div className="mx-auto flex max-w-5xl items-center justify-between gap-6 px-6 py-4">
          <Link to="/" className="font-heading text-lg font-semibold tracking-tight">
            Circle Downloader
          </Link>
          <nav className="flex items-center gap-1">
            {navItems.map(item => (
              <Link
                key={item.to}
                to={item.to}
                className="px-3 py-2 text-sm text-muted-foreground transition-colors hover:text-foreground [&.active]:text-foreground [&.active]:underline [&.active]:underline-offset-4"
                activeOptions={{ exact: item.to === '/' }}
              >
                {item.label}
              </Link>
            ))}
          </nav>
        </div>
      </header>
      <main className="mx-auto max-w-5xl px-6 py-8">
        <Outlet />
      </main>
    </div>
  )
}
