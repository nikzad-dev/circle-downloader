import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, waitFor } from '@testing-library/react'
import { StrictMode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { DownloadProgressProvider } from './download-progress'

const { invoke, listeners } = vi.hoisted(() => ({
  invoke: vi.fn(),
  listeners: new Set<number>(),
}))
vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class {
    onmessage = (_payload: unknown) => {}
  },
}))
describe('progress provider lifecycle', () => {
  it('keeps one native listener after StrictMode replay and removes it on unmount', async () => {
    let id = 0
    invoke.mockImplementation(
      async (command: string, args: { subscriptionId?: number }) => {
        if (command === 'subscribe_progress') {
          id += 1
          listeners.add(id)
          return id
        }
        if (command === 'unsubscribe_progress')
          listeners.delete(args.subscriptionId!)
        if (command === 'list_downloads') return []
      },
    )
    const client = new QueryClient()
    const view = render(
      <StrictMode>
        <QueryClientProvider client={client}>
          <DownloadProgressProvider>
            <p>Test page</p>
          </DownloadProgressProvider>
        </QueryClientProvider>
      </StrictMode>,
    )
    await waitFor(() => expect(listeners.size).toBe(1))
    expect(
      invoke.mock.calls.filter((call) => call[0] === 'subscribe_progress'),
    ).toHaveLength(2)
    view.rerender(
      <StrictMode>
        <QueryClientProvider client={client}>
          <DownloadProgressProvider>
            <p>Another page</p>
          </DownloadProgressProvider>
        </QueryClientProvider>
      </StrictMode>,
    )
    expect(listeners.size).toBe(1)
    view.unmount()
    await waitFor(() => expect(listeners.size).toBe(0))
    client.clear()
  })
})
