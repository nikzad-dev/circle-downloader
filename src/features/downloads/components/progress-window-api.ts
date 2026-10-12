import { invoke } from '@tauri-apps/api/core'
import { toast } from '@/components/ui/toast'

let opening: Promise<void> | undefined
export function openProgressWindow(): Promise<void> {
  opening ??= invoke<void>('open_progress_window')
    .catch(() => {
      toast.add({ title: 'Could not open download progress', type: 'error' })
    })
    .finally(() => {
      opening = undefined
    })
  return opening
}
export async function closeProgressWindow() {
  await invoke('close_progress_window')
}
