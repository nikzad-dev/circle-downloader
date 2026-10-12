import { Link, useLocation, useSearch } from '@tanstack/react-router'
import { FolderOpen } from 'lucide-react'
import { useMemo } from 'react'
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from '@/components/ui/accordion'
import { useQueues, useTaskOptions } from '@/features/advanced/api'
import {
  downloadCategories,
  resolvedCategory,
} from '@/features/advanced/categories'
import { useDownloads } from '@/features/downloads/api/downloads'
import { settingsSections } from '@/features/settings/lib/sections'

const linkClass =
  'flex min-h-9 items-center gap-2 rounded-md px-3 text-sm text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring [&.active]:bg-sidebar-accent [&.active]:font-medium [&.active]:text-foreground'

export function AppSidebar({
  onNavigate,
  id = 'download-sidebar',
}: {
  onNavigate?: () => void
  id?: string
}) {
  const { pathname } = useLocation()
  return pathname.startsWith('/settings') ? (
    <SettingsSidebar id={id} onNavigate={onNavigate} />
  ) : (
    <DownloadSidebar id={id} onNavigate={onNavigate} />
  )
}
function SettingsSidebar({
  id,
  onNavigate,
}: {
  id: string
  onNavigate?: () => void
}) {
  const search = useSearch({ strict: false })
  return (
    <aside
      id={id}
      aria-label="Settings sections"
      className="h-full overflow-y-auto bg-sidebar p-3"
    >
      <p className="px-3 py-3 text-xs font-semibold text-muted-foreground">
        Settings
      </p>
      <nav aria-label="Settings navigation" className="space-y-1">
        {settingsSections.map(({ id, label, icon: Icon }) => (
          <Link
            key={id}
            to="/settings"
            search={{ section: id }}
            onClick={onNavigate}
            aria-current={
              (search.section ?? 'general') === id ? 'page' : undefined
            }
            activeOptions={{ includeSearch: true }}
            className={`${linkClass} ${(search.section ?? 'general') === id ? 'active' : ''}`}
          >
            <Icon className="size-4" aria-hidden="true" />
            {label}
          </Link>
        ))}
      </nav>
    </aside>
  )
}
function DownloadSidebar({
  id,
  onNavigate,
}: {
  id: string
  onNavigate?: () => void
}) {
  const query = useDownloads()
  const controls = useTaskOptions()
  const queues = useQueues()
  const organization = useMemo(() => {
    const categories = new Map<string, number>(
      downloadCategories.map((category) => [category, 0]),
    )
    const tags = new Map<string, number>()
    const optionsById = new Map(
      (controls.data ?? []).map((options) => [options.id, options]),
    )
    for (const task of query.data ?? []) {
      const options = optionsById.get(task.id)
      const category = resolvedCategory(task.fileName, options)
      if (category)
        categories.set(category, (categories.get(category) ?? 0) + 1)
      for (const tag of options?.tags ?? [])
        tags.set(tag, (tags.get(tag) ?? 0) + 1)
    }
    return {
      categories: [...categories],
      tags: [...tags].sort(([a], [b]) => a.localeCompare(b)),
    }
  }, [controls.data, query.data])
  const { pathname } = useLocation()
  const search = useSearch({ strict: false })
  const inDownloads = pathname === '/' || pathname.startsWith('/downloads')
  return (
    <aside
      id={id}
      aria-label="Download filters"
      className="h-full overflow-y-auto bg-sidebar p-2"
    >
      <nav aria-label="Download categories" className="space-y-2">
        <div className="flex items-center gap-2 px-2.5 py-3 text-xs font-semibold text-muted-foreground">
          <FolderOpen className="size-4" aria-hidden="true" />
          Organize files
        </div>
        <Link
          to="/downloads"
          search={{ q: search.q, sort: search.sort, status: search.status }}
          onClick={onNavigate}
          className={`${linkClass} ${!search.queueId && !search.category && !search.tag ? 'active' : ''}`}
          aria-current={
            !search.queueId && !search.category && !search.tag
              ? 'page'
              : undefined
          }
        >
          All files
        </Link>
        <Accordion defaultValue={['queues', 'categories']}>
          <AccordionItem value="queues">
            <AccordionTrigger>Queues</AccordionTrigger>
            <AccordionContent className="space-y-0.5 pl-2">
              {queues.data?.map((queue) => (
                <Link
                  key={queue.id}
                  to="/downloads"
                  search={{ ...search, queueId: queue.id }}
                  onClick={onNavigate}
                  aria-current={
                    inDownloads && search.queueId === queue.id
                      ? 'page'
                      : undefined
                  }
                  className={`${linkClass} ${inDownloads && search.queueId === queue.id ? 'active' : ''}`}
                >
                  <span className="truncate">{queue.name}</span>
                  <span className="ml-auto text-xs tabular-nums">
                    {queue.taskCount}
                  </span>
                </Link>
              ))}
            </AccordionContent>
          </AccordionItem>
          {(['categories', 'tags'] as const).map((group) => (
            <AccordionItem key={group} value={group}>
              <AccordionTrigger>
                {group === 'categories' ? 'Categories' : 'Tags'}
              </AccordionTrigger>
              <AccordionContent className="space-y-0.5 pl-2">
                {organization[group].slice(0, 50).map(([label, count]) => (
                  <Link
                    key={label}
                    to="/downloads"
                    search={
                      group === 'categories'
                        ? { ...search, category: label }
                        : { ...search, tag: label }
                    }
                    onClick={onNavigate}
                    aria-current={
                      (group === 'categories'
                        ? search.category
                        : search.tag) === label
                        ? 'page'
                        : undefined
                    }
                    className={`${linkClass} ${inDownloads && (group === 'categories' ? search.category : search.tag) === label ? 'active' : ''}`}
                  >
                    <span className="truncate">{label}</span>
                    <span className="ml-auto text-xs tabular-nums">
                      {count}
                    </span>
                  </Link>
                ))}
                {organization[group].length === 0 && (
                  <p className="px-2 text-xs text-muted-foreground">
                    Assign {group} in download details.
                  </p>
                )}
                {organization[group].length > 50 && (
                  <p className="px-2 text-xs text-muted-foreground">
                    Showing the first 50. All assignments remain saved.
                  </p>
                )}
              </AccordionContent>
            </AccordionItem>
          ))}
        </Accordion>
      </nav>
    </aside>
  )
}
