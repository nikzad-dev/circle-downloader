import { invoke } from '@tauri-apps/api/core'
import { toast } from '@/components/ui/toast'

let opening: Promise<void> | undefined
export async function openDownloadWindow(
  initialUrl = '',
  bulk = false,
  initialUrls?: string[],
): Promise<void> {
  if (opening) {
    return initialUrl !== '' || (initialUrls?.length ?? 0) > 0
      ? opening.then(async () =>
          openDownloadWindow(initialUrl, bulk, initialUrls),
        )
      : opening
  }
  opening = invoke<void>('open_download_window', {
    initialUrl,
    bulk,
    ...(initialUrls ? { initialUrls } : {}),
  })
    .catch(() => {
      toast.add({ title: 'Could not open the download window', type: 'error' })
    })
    .finally(() => {
      opening = undefined
    })
  return opening
}
export async function closeDownloadWindow() {
  await invoke('close_download_window')
}
