import type { ProgressEvent } from '../orpc/schemas'
import { Channel, invoke } from '@tauri-apps/api/core'
import { progressEventSchema } from '../orpc/schemas'
import { normalizeNativeError } from './errors'

/** Disposal is effective immediately, even while native registration is pending. */
export function subscribeProgress(
  onEvent: (event: ProgressEvent) => void,
  onError: (error: unknown) => void,
) {
  let disposed = false
  let subscriptionId: number | undefined
  const channel = new Channel<unknown>()
  channel.onmessage = (payload) => {
    if (disposed) return
    const parsed = progressEventSchema.safeParse(payload)
    if (parsed.success) onEvent(parsed.data)
    else onError(new Error('Invalid progress event'))
  }
  const unregister = async () => {
    if (subscriptionId !== undefined) {
      const id = subscriptionId
      subscriptionId = undefined
      await invoke('unsubscribe_progress', { subscriptionId: id })
    }
  }
  const ready = invoke<number>('subscribe_progress', { onEvent: channel })
    .then(async (id) => {
      subscriptionId = id
      if (disposed) await unregister()
    })
    .catch(normalizeNativeError)
  return {
    ready,
    dispose: async () => {
      disposed = true
      channel.onmessage = () => {}
      await ready.catch(() => {})
      await unregister()
    },
  }
}
