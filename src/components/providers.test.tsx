import { render, screen } from '@testing-library/react'
import { expect, it, vi } from 'vitest'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => vi.fn()) }))
it('loads the download window without subscribing to or fetching full history', async () => {
  window.history.replaceState(null, '', '/?downloadWindow=1')
  invoke.mockImplementation(async (command: string) => {
    if (command === 'download_window_ready')
      return { initialUrl: '', bulk: false, generation: 0 }
    if (command === 'get_settings')
      return {
        maxConcurrentDownloads: 3,
        defaultDestinationId: 'default',
        autoStart: true,
        maxConnectionsPerDownload: 1,
      }
    if (command === 'list_destinations')
      return [
        {
          id: 'default',
          label: 'Downloads',
          displayPath: '/tmp/downloads',
          isDefault: true,
        },
      ]
    if (command === 'get_queue_policies')
      return [
        {
          id: 'default',
          name: 'Downloads',
          paused: false,
          maxConcurrent: 3,
          bytesPerSecond: 0,
          taskCount: 0,
        },
      ]
    if (command === 'list_credential_grants') return []
    throw new Error(`Unexpected command ${command}`)
  })
  try {
    const { AppProviders } = await import('./providers')
    render(<AppProviders />)
    expect(
      await screen.findByRole('textbox', { name: 'Download URL' }),
    ).toBeInTheDocument()
    expect(
      invoke.mock.calls.some(
        ([command]) =>
          command === 'list_downloads' || command === 'subscribe_progress',
      ),
    ).toBe(false)
  } finally {
    window.history.replaceState(null, '', '/')
  }
})
