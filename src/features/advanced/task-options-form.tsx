import type { TaskOptions } from './api'
import { useId, useState } from 'react'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { useGrants, useQueues } from './api'
import { downloadCategories } from './categories'

export const selectClass = 'h-9 w-full rounded-md border bg-background px-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring'
export function TaskOptionsForm({ value, onChange, disabled = false, automaticCategory, sections = { queue: true, organization: true, credentials: true } }: { automaticCategory?: string, sections?: { queue: boolean, organization: boolean, credentials: boolean, schedule?: boolean, category?: boolean }, value: TaskOptions, onChange: (value: TaskOptions) => void, disabled?: boolean }) {
  const id = useId()
  const [tagsText, setTagsText] = useState(value.tags.join(', '))
  const queues = useQueues()
  const grants = useGrants()
  const set = (patch: Partial<TaskOptions>) => onChange({ ...value, ...patch })
  return (
    <fieldset disabled={disabled} className="grid gap-3 min-[480px]:grid-cols-2">
      {sections.queue && (
        <>
          <div className="space-y-1">
            <Label htmlFor={`${id}-queue`}>Queue</Label>
            <select id={`${id}-queue`} className={selectClass} value={value.queueId} onChange={e => set({ queueId: e.target.value })}>{(queues.data ?? [{ id: 'default', name: 'Default' }]).map(q => <option key={q.id} value={q.id}>{q.name}</option>)}</select>
          </div>
          <div className="space-y-1">
            <Label htmlFor={`${id}-priority`}>Priority</Label>
            <select id={`${id}-priority`} className={selectClass} value={value.priority} onChange={e => set({ priority: Number(e.target.value) })}>{[['-2', 'Lowest'], ['-1', 'Low'], ['0', 'Normal'], ['1', 'High'], ['2', 'Highest']].map(([v, label]) => <option key={v} value={v}>{label}</option>)}</select>
          </div>
        </>
      )}
      {sections.organization && (
        <>
          {sections.category !== false && (
            <div className="space-y-1">
              <Label htmlFor={`${id}-category`}>Category</Label>
              <CategorySelect value={value.category} onChange={category => set({ category })} automaticCategory={automaticCategory} id={`${id}-category`} />
            </div>
          )}
          <div className="space-y-1">
            <Label htmlFor={`${id}-tags`}>Tags (comma separated)</Label>
            <Input
              id={`${id}-tags`}
              value={tagsText}
              onChange={(e) => {
                setTagsText(e.target.value)
                set({ tags: e.target.value.split(',').map(v => v.trim()).filter(Boolean) })
              }}
            />
          </div>
        </>
      )}
      {sections.schedule !== false && (
        <Label className="flex items-center gap-2 min-[480px]:col-span-2">
          <input
            type="checkbox"
            checked={value.schedule !== null}
            onChange={(e) => {
              if (!e.target.checked) {
                set({ schedule: null, scheduledAt: null })
                return
              }
              const now = new Date()
              const local = new Date(now.getTime() - now.getTimezoneOffset() * 60_000).toISOString().slice(0, 16)
              set({ schedule: { localTime: local, timeZone: new Intl.DateTimeFormat().resolvedOptions().timeZone, ambiguity: 'earlier' }, scheduledAt: null })
            }}
          />
          Schedule start
        </Label>
      )}
      {sections.schedule !== false && value.schedule !== null && (
        <div className="space-y-1">
          <Label htmlFor={`${id}-time`}>Start no earlier than</Label>
          <Input id={`${id}-time`} type="datetime-local" disabled={value.schedule === null} value={value.schedule?.localTime ?? ''} onChange={e => set({ schedule: e.target.value ? { localTime: e.target.value, timeZone: value.schedule?.timeZone ?? new Intl.DateTimeFormat().resolvedOptions().timeZone, ambiguity: value.schedule?.ambiguity ?? 'earlier' } : null, scheduledAt: null })} />
        </div>
      )}
      {sections.schedule !== false && value.schedule !== null && (
        <>
          <div className="space-y-1">
            <Label htmlFor={`${id}-zone`}>IANA time zone</Label>
            <Input id={`${id}-zone`} value={value.schedule.timeZone} onChange={e => set({ schedule: { ...value.schedule!, timeZone: e.target.value } })} />
          </div>
          <div className="space-y-1">
            <Label htmlFor={`${id}-fold`}>If clocks repeat this time</Label>
            <select id={`${id}-fold`} className={selectClass} value={value.schedule.ambiguity} onChange={e => set({ schedule: { ...value.schedule!, ambiguity: e.target.value as 'earlier' | 'later' } })}>
              <option value="earlier">Earlier occurrence</option>
              <option value="later">Later occurrence</option>
            </select>
          </div>
          <p className="text-xs text-muted-foreground min-[480px]:col-span-2">The app must be running. Skipped daylight-saving times are rejected; overdue work starts on reopen.</p>
        </>
      )}
      {sections.credentials && (
        <div className="space-y-1 min-[480px]:col-span-2">
          <Label htmlFor={`${id}-credential`}>Origin authorization</Label>
          <select id={`${id}-credential`} className={selectClass} value={value.credentialId ?? ''} onChange={e => set({ credentialId: e.target.value || null })}>
            <option value="">No credentials</option>
            {grants.data?.filter(g => g.kind === 'origin').map(g => <option key={g.id} value={g.id}>{g.scope}</option>)}
          </select>
        </div>
      )}
      {queues.isError && <p role="alert" className="text-xs text-destructive min-[480px]:col-span-2">Queues could not be loaded. Retry from the Queues page.</p>}
    </fieldset>
  )
}

export function CategorySelect({ value, onChange, automaticCategory, id }: { value: string, onChange: (category: string) => void, automaticCategory?: string, id: string }) {
  return (
    <select id={id} className={selectClass} value={value} onChange={e => onChange(e.target.value)}>
      <option value="">
        Automatic
        {(automaticCategory ?? '') !== '' ? ` (${automaticCategory})` : ' (by file type)'}
      </option>
      {downloadCategories.map(category => <option key={category} value={category}>{category}</option>)}
      {value && !downloadCategories.includes(value) && (
        <option value={value}>
          {value}
          {' '}
          (saved category)
        </option>
      )}
    </select>
  )
}
