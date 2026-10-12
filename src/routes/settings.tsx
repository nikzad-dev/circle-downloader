import { createFileRoute } from '@tanstack/react-router'
import { settingsSectionSchema } from '@/features/settings/lib/sections'
import { SettingsPage } from '@/features/settings/pages/settings-page'

export const Route = createFileRoute('/settings')({
  validateSearch: (search: Record<string, unknown>) => ({
    section: settingsSectionSchema.catch('general').parse(search.section),
  }),
  component: SettingsRoute,
})

function SettingsRoute() {
  const { section } = Route.useSearch()
  return <SettingsPage section={section} />
}
