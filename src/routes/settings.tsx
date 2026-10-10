import { createFileRoute } from '@tanstack/react-router'
import { ThemeSelector } from '@/components/theme/theme-selector'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'

export const Route = createFileRoute('/settings')({ component: SettingsPage })
function SettingsPage() {
  return (
    <section className="max-w-2xl space-y-6">
      <div>
        <p className="mb-2 text-xs font-semibold tracking-widest text-muted-foreground uppercase">Make it yours</p>
        <h1 className="font-heading text-3xl font-semibold tracking-tight">Settings</h1>
      </div>
      <Card>
        <CardHeader>
          <CardTitle>Appearance</CardTitle>
          <CardDescription>Choose a look, or follow your system’s preference.</CardDescription>
        </CardHeader>
        <CardContent><ThemeSelector /></CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Download location</CardTitle>
          <CardDescription>Files are saved in Circle’s app-managed download folder.</CardDescription>
        </CardHeader>
        <CardContent className="text-sm leading-relaxed text-muted-foreground">Folder selection and transfer preferences are planned for a later update.</CardContent>
      </Card>
    </section>
  )
}
