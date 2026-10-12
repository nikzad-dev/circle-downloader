import { beforeEach, expect, it, vi } from 'vitest'
import { openProgressWindow } from './progress-window-api'
const { invoke, notify } = vi.hoisted(() => ({
  invoke: vi.fn(),
  notify: vi.fn(),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@/components/ui/toast', () => ({ toast: { add: notify } }))
beforeEach(() => {
  invoke.mockReset()
  notify.mockReset()
})
it('coalesces repeated requests and allows opening again after completion', async () => {
  let complete!: () => void
  invoke
    .mockImplementationOnce(
      async () =>
        new Promise<void>((resolve) => {
          complete = resolve
        }),
    )
    .mockResolvedValue(undefined)
  const first = openProgressWindow(),
    second = openProgressWindow()
  expect(invoke).toHaveBeenCalledTimes(1)
  complete()
  await Promise.all([first, second])
  await openProgressWindow()
  expect(invoke).toHaveBeenCalledTimes(2)
})
it('reports a window failure without turning a successful download into a failed submit', async () => {
  invoke.mockRejectedValue(new Error('Unavailable'))
  await expect(openProgressWindow()).resolves.toBeUndefined()
  expect(notify).toHaveBeenCalledWith({
    title: 'Could not open download progress',
    type: 'error',
  })
})
