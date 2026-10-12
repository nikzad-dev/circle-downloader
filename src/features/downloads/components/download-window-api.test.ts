import { expect, it, vi } from 'vitest'
import { openDownloadWindow } from './download-window-api'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
it('keeps a second pasted link while the first open is still pending', async () => {
  let finish: (() => void) | undefined
  invoke
    .mockImplementationOnce(
      async () =>
        new Promise<void>((resolve) => {
          finish = resolve
        }),
    )
    .mockResolvedValue(undefined)
  const first = openDownloadWindow('https://example.com/first')
  const second = openDownloadWindow('https://example.com/second')
  expect(invoke).toHaveBeenCalledTimes(1)
  finish?.()
  await Promise.all([first, second])
  expect(invoke).toHaveBeenLastCalledWith('open_download_window', {
    initialUrl: 'https://example.com/second',
    bulk: false,
  })
})
