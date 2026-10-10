import { describe, expect, it, vi } from 'vitest'
import { progress } from '@/test/fixtures'
import { subscribeProgress } from './progress'

const { invoke, channels } = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as { onmessage: (payload: unknown) => void }[] }))
vi.mock('@tauri-apps/api/core', () => ({ invoke, Channel: class {
  onmessage = (_payload: unknown) => {}
  constructor() { channels.push(this) }
} }))
describe('native subscription lifecycle', () => {
  it('disposes pending registration and rejects late messages', async () => {
    let resolve!: (id: number) => void
    invoke.mockImplementationOnce(async () => new Promise((done) => {
      resolve = done
    })).mockResolvedValue(undefined)
    const receive = vi.fn()
    const subscription = subscribeProgress(receive, vi.fn())
    const channel = channels[channels.length - 1]
    const dispose = subscription.dispose()
    channel.onmessage(progress())
    resolve(42)
    await dispose
    expect(receive).not.toHaveBeenCalled()
    expect(invoke).toHaveBeenCalledWith('unsubscribe_progress', { subscriptionId: 42 })
  })
  it('validates payloads without throwing from native callbacks', async () => {
    invoke.mockResolvedValue(1)
    const receive = vi.fn()
    const error = vi.fn()
    const subscription = subscribeProgress(receive, error)
    await subscription.ready
    channels[channels.length - 1].onmessage({ bad: true })
    expect(error).toHaveBeenCalledTimes(1)
    channels[channels.length - 1].onmessage(progress())
    expect(receive).toHaveBeenCalledWith(progress())
    await subscription.dispose()
  })
})
