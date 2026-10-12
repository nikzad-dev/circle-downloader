import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ThemeProvider } from '@/components/theme/theme-provider'
import { updateSettingsInputSchema } from '@/lib/orpc/schemas'
import { SettingsPage } from './settings-page'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))

beforeEach(() => {
  invoke.mockReset()
  let settings = {
    maxConcurrentDownloads: 3,
    maxConnectionsPerDownload: 1,
    defaultDestinationId: 'default',
    autoStart: true,
  }
  invoke.mockImplementation(
    async (command: string, args?: { input?: unknown }) => {
      if (command === 'get_settings') return Promise.resolve(settings)
      if (command === 'list_destinations')
        return Promise.resolve([
          {
            id: 'default',
            label: 'App-managed downloads',
            displayPath: '/tmp/downloads',
            isDefault: true,
          },
        ])
      if (command === 'update_settings') {
        settings = {
          ...settings,
          ...updateSettingsInputSchema.parse(args?.input),
        }
        return Promise.resolve(settings)
      }
      return Promise.reject(new Error(command))
    },
  )
})
async function setup() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  render(
    <ThemeProvider>
      <QueryClientProvider client={client}>
        <SettingsPage section="downloads" />
      </QueryClientProvider>
    </ThemeProvider>,
  )
  const input = await screen.findByRole('spinbutton', {
    name: 'Connections per download',
  })
  return { input, user: userEvent.setup() }
}

describe('optional connection setting', () => {
  it('defaults to one and explains compatible-origin acceleration', async () => {
    const { input } = await setup()
    expect(input).toHaveValue(1)
    expect(screen.getByText(/Optional acceleration/)).toBeInTheDocument()
    expect(invoke).not.toHaveBeenCalledWith(
      'update_settings',
      expect.anything(),
    )
  })
  it.each([
    ['9', 4],
    ['0', 1],
    ['2.7', 2],
  ])('saves %s as a bounded integer (%s)', async (value, expected) => {
    const { input, user } = await setup()
    await user.clear(input)
    await user.type(input, value)
    await user.tab()
    if (expected !== 1) {
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith('update_settings', {
          input: { maxConnectionsPerDownload: expected },
        }),
      )
    } else {
      expect(invoke).not.toHaveBeenCalledWith(
        'update_settings',
        expect.anything(),
      )
    }
    await waitFor(() => expect(input).toHaveValue(expected))
  })
})
