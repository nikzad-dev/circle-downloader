import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/settings')({
  component: SettingsPage,
})

function SettingsPage() {
  return (
    <section className="space-y-4">
      <h1 className="font-heading text-2xl font-semibold tracking-tight">
        Settings
      </h1>
      <p className="text-muted-foreground">
        App preferences will live here.
      </p>
    </section>
  )
}
