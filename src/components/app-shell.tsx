import { Link, Outlet } from '@tanstack/react-router'
import { Activity, ArrowDownToLine, Circle, Home, Plus, Settings } from 'lucide-react'
import { useRef, useState } from 'react'
import { ThemeSelector } from '@/components/theme/theme-selector'
import { Button } from '@/components/ui/button'
import { AddDownloadContext } from '@/features/downloads/components/add-download-context'
import { AddDownloadDialog } from '@/features/downloads/components/add-download-dialog'

const navItems = [
  { to: '/', label: 'Home', icon: Home },
  { to: '/downloads', label: 'Downloads', icon: ArrowDownToLine },
  { to: '/settings', label: 'Settings', icon: Settings },
  { to: '/health_check', label: 'Health', icon: Activity },
] as const
export function AppShell() {
  const [open, setOpen] = useState(false)
  const triggerRef = useRef<HTMLElement | null>(null)
  const add = () => {
    triggerRef.current = document.activeElement as HTMLElement
    setOpen(true)
  }
  return (
    <AddDownloadContext value={add}>
      <div className="flex min-h-svh bg-background text-foreground">
        <a href="#main-content" className="sr-only z-50 bg-background p-3 focus:not-sr-only focus:fixed">Skip to content</a>
        <aside className="sticky top-0 hidden h-svh w-44 shrink-0 flex-col border-r bg-sidebar p-4 md:flex lg:w-56 lg:p-6">
          <Link to="/" className="mb-10 flex items-center gap-2 font-heading text-xl font-semibold tracking-tight">
            <Circle className="size-5 text-primary" strokeWidth={3} aria-hidden="true" />
            Circle
            <span className="sr-only">Downloader</span>
          </Link>
          <p className="mb-3 px-3 text-[10px] font-semibold tracking-widest text-muted-foreground uppercase">Workspace</p>
          <nav aria-label="Main navigation" className="space-y-1">
            {navItems.map(({ to, label, icon: Icon }) => (
              <Link key={to} to={to} activeOptions={{ exact: to === '/' }} activeProps={{ 'aria-current': 'page' }} className="flex items-center gap-3 px-3 py-3 text-sm text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring [&.active]:bg-sidebar-accent [&.active]:font-medium [&.active]:text-foreground">
                <Icon className="size-4" aria-hidden="true" />
                {label}
              </Link>
            ))}
          </nav>
          <div className="mt-auto border-t pt-5">
            <p className="mb-3 text-[10px] font-semibold tracking-widest text-muted-foreground uppercase">Appearance</p>
            <ThemeSelector />
          </div>
        </aside>
        <div className="min-w-0 flex-1">
          <header className="sticky top-0 z-20 flex min-h-20 bg-background flex-wrap items-center justify-between gap-3 border-b px-4 py-4 sm:px-6 lg:px-8">
            <Link to="/" className="flex items-center gap-2 font-heading text-lg font-semibold md:hidden">
              <Circle className="size-4 text-primary" aria-hidden="true" />
              Circle
            </Link>
            <p className="hidden text-xs font-medium tracking-widest text-muted-foreground uppercase md:block">A home for your downloads</p>
            <Button onClick={add} size="sm">
              <Plus aria-hidden="true" />
              Add download
            </Button>
            <nav aria-label="Main navigation" className="flex w-full flex-wrap gap-1 md:hidden">{navItems.map(({ to, label }) => <Link key={to} to={to} activeProps={{ 'aria-current': 'page' }} activeOptions={{ exact: to === '/' }} className="px-3 py-2 text-sm text-muted-foreground [&.active]:bg-muted [&.active]:text-foreground">{label}</Link>)}</nav>
          </header>
          <main id="main-content" tabIndex={-1} className="mx-auto max-w-6xl px-4 py-6 outline-none sm:px-6 lg:px-8 lg:py-8"><Outlet /></main>
        </div>
      </div>
      <AddDownloadDialog open={open} onOpenChange={setOpen} triggerRef={triggerRef} />
    </AddDownloadContext>
  )
}
