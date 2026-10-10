import { Link, Outlet } from '@tanstack/react-router'
import { Circle, Plus } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { AppMenubar } from '@/components/app-menubar'
import { workspacePages } from '@/components/app-navigation'
import { Button } from '@/components/ui/button'
import { AddDownloadContext } from '@/features/downloads/components/add-download-context'
import { AddDownloadDialog } from '@/features/downloads/components/add-download-dialog'
import { strings } from '@/lib/ui-strings'

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
  const addButtonRef = useRef<HTMLButtonElement | null>(null)
  const add = (url = '', trigger = document.activeElement as HTMLElement | null) => {
    triggerRef.current = trigger
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
      <div className="min-h-svh bg-background text-foreground">
        <a href="#main-content" className="sr-only z-50 bg-background p-3 focus:not-sr-only focus:fixed">Skip to content</a>
        <header className="sticky top-0 z-20 border-b bg-background">
          <div className="mx-auto flex max-w-6xl items-center gap-2 px-4 py-3 sm:gap-5 sm:px-6 lg:px-8">
            <Link to="/" activeOptions={{ exact: true }} aria-label="Circle Downloader home" className="flex shrink-0 items-center gap-2 font-heading text-lg font-semibold tracking-tight focus-visible:outline-2 focus-visible:outline-ring">
              <Circle className="size-5 text-primary" strokeWidth={3} aria-hidden="true" />
              <span className="hidden sm:inline">{strings.appName}</span>
            </Link>
            <AppMenubar onAdd={() => add('', addButtonRef.current)} dialogOpen={open} />
            <Button ref={addButtonRef} onClick={() => add()} size="sm" className="ml-auto shrink-0">
              <Plus aria-hidden="true" />
              <span className="sr-only sm:not-sr-only">{strings.addDownload}</span>
            </Button>
          </div>
          <nav aria-label="Main navigation" className="mx-auto flex max-w-6xl gap-1 px-4 sm:px-6 lg:px-8">
            {workspacePages.map(({ to, label, icon: Icon }) => (
              <Link key={to} to={to} activeOptions={{ exact: to === '/' }} activeProps={{ 'aria-current': 'page', 'className': 'active' }} className="flex min-h-11 items-center gap-2 border-b-2 border-transparent px-2 text-sm sm:px-3 text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring [&.active]:border-primary [&.active]:font-medium [&.active]:text-foreground">
                <Icon className="size-4" aria-hidden="true" />
                {label}
              </Link>
            ))}
          </nav>
        </header>
        <main id="main-content" tabIndex={-1} className="mx-auto min-w-0 max-w-6xl px-4 py-6 outline-none sm:px-6 lg:px-8 lg:py-8"><Outlet /></main>
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
