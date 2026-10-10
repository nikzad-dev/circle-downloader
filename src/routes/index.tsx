import { createFileRoute, Link } from '@tanstack/react-router'
import { Button } from '#components/ui/button'

export const Route = createFileRoute('/')({
  component: HomePage,
})

function HomePage() {
  return (
    <section className="space-y-6">
      <div className="space-y-2">
        <h1 className="font-heading text-3xl font-semibold tracking-tight">
          Welcome
        </h1>
        <p className="max-w-xl text-muted-foreground">
          Manage downloads from one place. Open the downloads page to see progress,
          or adjust preferences in settings.
        </p>
      </div>
      <div className="flex flex-wrap gap-3">
        <Button render={<Link to="/downloads" />}>
          View downloads
        </Button>
        <Button variant="outline" render={<Link to="/settings" />}>
          Settings
        </Button>
      </div>
    </section>
  )
}
