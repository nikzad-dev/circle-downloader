import { Channel, invoke } from '@tauri-apps/api/core'
import { progressEventSchema, type ProgressEvent } from '../orpc/schemas'
import { normalizeNativeError } from './errors'

/**
 * Subscribe to backend progress events. UI wiring is deferred;
 * this helper keeps the Channel contract typed for later phases.
 */
export async function subscribeProgress(
  onEvent: (event: ProgressEvent) => void,
): Promise<void> {
  const channel = new Channel<unknown>()
  channel.onmessage = (payload) => {
    onEvent(progressEventSchema.parse(payload))
  }

  try {
    await invoke('subscribe_progress', { onEvent: channel })
  }
  catch (error) {
    normalizeNativeError(error)
  }
}
