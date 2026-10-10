import { Link, Outlet } from '@tanstack/react-router'
import { Activity, ArrowDownToLine, Circle, Home, Plus, Settings } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { ThemeSelector } from '@/components/theme/theme-selector'
import { Button } from '@/components/ui/button'
import { Kbd } from '@/components/ui/kbd'
import { AddDownloadContext } from '@/features/downloads/components/add-download-context'
import { AddDownloadDialog } from '@/features/downloads/components/add-download-dialog'
import { strings } from '@/lib/ui-strings'

const navItems = [
  { to: '/', label: 'Home', icon: Home },
  { to: '/downloads', label: 'Downloads', icon: ArrowDownToLine },
  { to: '/settings', label: 'Settings', icon: Settings },
  { to: '/health_check', label: 'Health', icon: Activity },
] as const

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement))
    return false
  const tag = target.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || target.isContentEditable
}

export function AppShell() {
  const [open, setOpen] = useState(false)
  const [initialUrl, setInitialUrl] = useState('')
  const triggerRef = useRef<HTMLElement | null>(null)
  const add = (url = '') => {
    triggerRef.current = document.activeElement as HTMLElement
    setInitialUrl(url)
    setOpen(true)
  }
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey
      if (mod && event.key.toLowerCase() === 'n') {
        event.preventDefault()
        add()
        return
      }
      if (mod && event.key.toLowerCase() === 'v' && !isEditableTarget(event.target)) {
        event.preventDefault()
        void navigator.clipboard.readText().then((text) => {
          const value = text.trim()
          if (/^https?:\/\//i.test(value))
            add(value)
          else
            add()
        }).catch(() => add())
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])
  return (
    <AddDownloadContext value={() => add()}>
      <div className="flex min-h-svh bg-background text-foreground">
        <a href="#main-content" className="sr-only z-50 bg-background p-3 focus:not-sr-only focus:fixed">Skip to content</a>
        <aside className="sticky top-0 hidden h-svh w-44 shrink-0 flex-col border-r bg-sidebar p-4 md:flex lg:w-56 lg:p-6">
          <Link to="/" className="mb-10 flex items-center gap-2 font-heading text-xl font-semibold tracking-tight">
            <Circle className="size-5 text-primary" strokeWidth={3} aria-hidden="true" />
            {strings.appName}
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
          <div className="mt-auto space-y-4 border-t pt-5">
            <p className="text-[10px] font-semibold tracking-widest text-muted-foreground uppercase">Shortcuts</p>
            <p className="flex items-center gap-2 text-xs text-muted-foreground">
              <Kbd>⌘</Kbd>
              <Kbd>N</Kbd>
              <span>{strings.shortcutAdd}</span>
            </p>
            <p className="text-[10px] font-semibold tracking-widest text-muted-foreground uppercase">Appearance</p>
            <ThemeSelector />
          </div>
        </aside>
        <div className="min-w-0 flex-1">
          <header className="sticky top-0 z-20 flex min-h-20 flex-wrap items-center justify-between gap-3 border-b bg-background px-4 py-4 sm:px-6 lg:px-8">
            <Link to="/" className="flex items-center gap-2 font-heading text-lg font-semibold md:hidden">
              <Circle className="size-4 text-primary" aria-hidden="true" />
              {strings.appName}
            </Link>
            <p className="hidden text-xs font-medium tracking-widest text-muted-foreground uppercase md:block">A home for your downloads</p>
            <Button onClick={() => add()} size="sm">
              <Plus aria-hidden="true" />
              {strings.addDownload}
            </Button>
            <nav aria-label="Main navigation" className="flex w-full flex-wrap gap-1 md:hidden">{navItems.map(({ to, label }) => <Link key={to} to={to} activeProps={{ 'aria-current': 'page' }} activeOptions={{ exact: to === '/' }} className="px-3 py-2 text-sm text-muted-foreground [&.active]:bg-muted [&.active]:text-foreground">{label}</Link>)}</nav>
          </header>
          <main id="main-content" tabIndex={-1} className="mx-auto max-w-6xl px-4 py-6 outline-none sm:px-6 lg:px-8 lg:py-8"><Outlet /></main>
        </div>
      </div>
      <AddDownloadDialog
        open={open}
        onOpenChange={(next) => {
          setOpen(next)
          if (!next)
            setInitialUrl('')
        }}
        triggerRef={triggerRef}
        initialUrl={initialUrl}
      />
    </AddDownloadContext>
  )
}
