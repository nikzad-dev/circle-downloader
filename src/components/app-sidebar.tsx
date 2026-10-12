import { Link, useLocation, useSearch } from '@tanstack/react-router'
import {
  ArrowDown,
  CheckCheck,
  CircleAlert,
  Clock,
  Download,
  LoaderCircle,
  Pause,
  ShieldCheck,
  X,
} from 'lucide-react'
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
import { statusLabels } from '@/features/downloads/lib/format'

const categories = [
  { status: 'all', label: 'All downloads', icon: Download },
  { status: 'downloading', label: statusLabels.downloading, icon: ArrowDown },
  { status: 'queued', label: statusLabels.queued, icon: Clock },
  { status: 'connecting', label: statusLabels.connecting, icon: LoaderCircle },
  { status: 'paused', label: statusLabels.paused, icon: Pause },
  { status: 'verifying', label: statusLabels.verifying, icon: ShieldCheck },
  { status: 'completed', label: statusLabels.completed, icon: CheckCheck },
  { status: 'failed', label: statusLabels.failed, icon: CircleAlert },
  { status: 'canceled', label: statusLabels.canceled, icon: X },
] as const

const linkClass =
  'flex min-h-8 items-center gap-2 rounded-md px-2.5 text-sm text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring [&.active]:bg-sidebar-accent [&.active]:font-medium [&.active]:text-foreground'

export function AppSidebar({
  onNavigate,
  id = 'download-sidebar',
}: {
  onNavigate?: () => void
  id?: string
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
  const inDownloads = pathname.startsWith('/downloads')
  const selected = search.status ?? 'all'
  const selectedGroup = ['completed', 'failed', 'canceled'].includes(selected)
    ? 'history'
    : 'transfers'
  const categoryLink = ({
    status,
    label,
    icon: Icon,
  }: (typeof categories)[number]) => {
    const active = inDownloads && selected === status
    const count = query.data?.filter(
      (task) => status === 'all' || task.status === status,
    ).length
    return (
      <Link
        key={status}
        to="/downloads"
        search={{
          q: search.q,
          sort: search.sort,
          status: status === 'all' ? undefined : status,
        }}
        onClick={onNavigate}
        aria-current={active ? 'page' : undefined}
        className={`${linkClass} ${active ? 'active' : ''}`}
      >
        <Icon className="size-4 shrink-0" aria-hidden="true" />
        <span className="truncate">{label}</span>
        <span className="ml-auto text-xs text-muted-foreground tabular-nums">
          {count ?? '—'}
        </span>
      </Link>
    )
  }
  return (
    <aside
      id={id}
      aria-label="Workspace sidebar"
      className="h-full overflow-y-auto bg-sidebar p-2"
    >
      <nav aria-label="Download categories" className="space-y-2">
        {categoryLink(categories[0])}
        <Accordion key={selectedGroup} defaultValue={[selectedGroup]}>
          {[
            {
              value: 'transfers',
              label: 'Transfers',
              statuses: [
                'downloading',
                'queued',
                'connecting',
                'paused',
                'verifying',
              ],
            },
            {
              value: 'history',
              label: 'History',
              statuses: ['completed', 'failed', 'canceled'],
            },
          ].map((group) => (
            <AccordionItem key={group.value} value={group.value}>
              <AccordionTrigger>{group.label}</AccordionTrigger>
              <AccordionContent className="space-y-0.5 pl-2">
                {categories
                  .filter((category) =>
                    group.statuses.includes(category.status),
                  )
                  .map(categoryLink)}
              </AccordionContent>
            </AccordionItem>
          ))}
        </Accordion>
        <Accordion defaultValue={['categories']}>
          <AccordionItem value="queues">
            <AccordionTrigger>Queues</AccordionTrigger>
            <AccordionContent className="space-y-0.5 pl-2">
              {queues.data?.map((queue) => (
                <Link
                  key={queue.id}
                  to="/downloads"
                  search={{ queueId: queue.id }}
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
              <Link to="/queues" onClick={onNavigate} className={linkClass}>
                Manage queues
              </Link>
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
                        ? { category: label }
                        : { tag: label }
                    }
                    onClick={onNavigate}
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
                    Assign
                    {group} in download details.
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
