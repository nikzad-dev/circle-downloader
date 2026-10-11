import { Link, Outlet } from '@tanstack/react-router'
import { Circle, PanelLeft, Plus } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useDefaultLayout } from 'react-resizable-panels'
import { AppMenubar } from '@/components/app-menubar'
import { AppSidebar } from '@/components/app-sidebar'
import { Button } from '@/components/ui/button'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable'
import { Sheet, SheetContent, SheetTitle, SheetTrigger } from '@/components/ui/sheet'
import { toast } from '@/components/ui/toast'
import { BulkImportButton } from '@/features/advanced/bulk-import'
import { AddDownloadContext } from '@/features/downloads/components/add-download-context'
import { openDownloadWindow } from '@/features/downloads/components/download-window-api'
import { pastedLinks } from '@/features/downloads/lib/pasted-links'
import { strings } from '@/lib/ui-strings'

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement))
    return false
  return target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"], [role="dialog"], [role="alertdialog"]') !== null
}

export function AppShell() {
  const [sidebarOpen, setSidebarOpen] = useState(true)
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false)
  const { defaultLayout, onLayoutChanged } = useDefaultLayout({ id: 'circle-workspace', panelIds: ['sidebar', 'content'], onlySaveAfterUserInteractions: true })
  const add = (url = '') => {
    void openDownloadWindow(url)
  }
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey
      if (mod && event.key.toLowerCase() === 'n') {
        event.preventDefault()
        add()
      }
    }
    const onPaste = (event: ClipboardEvent) => {
      if (event.defaultPrevented || event.composedPath().some(isEditableTarget))
        return
      const links = pastedLinks(event.clipboardData?.getData('text/plain') ?? '')
      if (links.length === 0)
        return
      event.preventDefault()
      if (links.length > 200 || links.some(link => link.length > 8192)) {
        toast.add({ title: 'Too many or oversized links', description: 'Paste up to 200 links, each no longer than 8,192 characters.', type: 'error' })
        return
      }
      void openDownloadWindow(links.length === 1 ? links[0] : '', links.length > 1, links.length > 1 ? links : undefined)
    }
    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('paste', onPaste)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('paste', onPaste)
    }
  }, [])
  return (
    <AddDownloadContext value={() => add()}>
      <div className="flex h-svh flex-col overflow-hidden bg-background text-foreground">
        <a href="#main-content" className="sr-only z-50 bg-background p-3 focus:not-sr-only focus:fixed">Skip to content</a>
        <header className="z-20 flex h-12 shrink-0 items-center gap-2 border-b bg-background px-3 sm:px-4">
          <Button variant="ghost" size="icon-sm" className="hidden md:inline-flex" aria-label={sidebarOpen ? 'Hide sidebar' : 'Show sidebar'} aria-expanded={sidebarOpen} aria-controls="download-sidebar" onClick={() => setSidebarOpen(value => !value)}>
            <PanelLeft className="size-[18px]" aria-hidden="true" />
          </Button>
          <Sheet open={mobileSidebarOpen} onOpenChange={setMobileSidebarOpen}>
            <SheetTrigger render={<Button variant="ghost" size="icon-sm" className="md:hidden" aria-label="Show sidebar" />}>
              <PanelLeft className="size-[18px]" aria-hidden="true" />
            </SheetTrigger>
            <SheetContent side="left" className="w-64 max-w-[85%] gap-0">
              <SheetTitle className="px-6 py-5 text-sm tracking-normal normal-case">Workspace</SheetTitle>
              <AppSidebar id="mobile-download-sidebar" onNavigate={() => setMobileSidebarOpen(false)} />
            </SheetContent>
          </Sheet>
          <Link to="/" activeOptions={{ exact: true }} aria-label="Circle Downloader home" className="flex shrink-0 items-center gap-2 font-heading text-sm font-semibold tracking-tight focus-visible:outline-2 focus-visible:outline-ring">
            <Circle className="size-4 text-sidebar-primary" strokeWidth={3} aria-hidden="true" />
            <span className="hidden sm:inline">{strings.appName}</span>
          </Link>
          <AppMenubar />
          <Button onClick={() => add()} size="sm" className="ml-auto shrink-0 rounded-md">
            <Plus className="size-4" aria-hidden="true" />
            <span className="sr-only sm:not-sr-only">{strings.addDownload}</span>
          </Button>
          <BulkImportButton />
        </header>
        <div className="relative flex min-h-0 flex-1">
          <ResizablePanelGroup className="workspace-panels" orientation="horizontal" defaultLayout={defaultLayout} onLayoutChanged={sidebarOpen ? onLayoutChanged : undefined}>
            {sidebarOpen && (
              <>
                <ResizablePanel id="sidebar" defaultSize={216} minSize={180} maxSize={320} className="hidden md:block">
                  <AppSidebar />
                </ResizablePanel>
                <ResizableHandle aria-label="Resize download sidebar" className="hidden transition-colors hover:bg-ring md:flex" />
              </>
            )}
            <ResizablePanel id="content" minSize={0}>
              <main id="main-content" tabIndex={-1} className="h-full min-w-0 overflow-auto p-2.5 outline-none sm:p-3"><Outlet /></main>
            </ResizablePanel>
          </ResizablePanelGroup>
        </div>
      </div>
    </AddDownloadContext>
  )
}
