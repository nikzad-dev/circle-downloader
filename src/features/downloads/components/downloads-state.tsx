import { Download, Loader2 } from 'lucide-react'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { useAddDownload } from './add-download-context'

export function DownloadsEmpty() {
  const add = useAddDownload()
  return (
    <Empty className="min-h-64 flex-1 border-0">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <Download aria-hidden="true" />
        </EmptyMedia>
        <EmptyTitle>Your next download starts here</EmptyTitle>
        <EmptyDescription>
          Paste a download link here or choose Add download. Use the filters on
          the left to organize your files.
        </EmptyDescription>
      </EmptyHeader>
      <Button onClick={add}>Add your first download</Button>
    </Empty>
  )
}
export function DownloadsLoading() {
  return (
    <div
      role="status"
      className="flex min-h-48 items-center justify-center gap-3 text-sm text-muted-foreground"
    >
      <Loader2 className="size-4 animate-spin" aria-hidden="true" />
      Loading downloads…
    </div>
  )
}
export function DownloadsError({
  retry,
  pending,
  background = false,
}: {
  retry: () => void
  pending: boolean
  background?: boolean
}) {
  return (
    <Alert variant="destructive">
      <AlertTitle>
        {background
          ? 'Could not refresh downloads'
          : 'Could not load downloads'}
      </AlertTitle>
      <AlertDescription>
        <p>
          {background
            ? 'Showing the last available list. Try refreshing.'
            : 'Open Circle’s desktop app and try again.'}
        </p>
        <Button
          className="mt-3"
          variant="outline"
          size="sm"
          disabled={pending}
          onClick={retry}
        >
          {pending ? 'Refreshing…' : 'Try again'}
        </Button>
      </AlertDescription>
    </Alert>
  )
}
