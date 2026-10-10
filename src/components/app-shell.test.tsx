import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, createRouter, RouterProvider } from '@tanstack/react-router'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { downloadsKey, mergeTask } from '@/features/downloads/api/downloads'
import { routeTree } from '@/routeTree.gen'
import { task } from '@/test/fixtures'
import { ThemeProvider } from './theme/theme-provider'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
async function setup(path = '/') {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const router = createRouter({ routeTree, history: createMemoryHistory({ initialEntries: [path] }), context: { queryClient: client } })
  await router.load()
  const view = render(<ThemeProvider><QueryClientProvider client={client}><RouterProvider router={router} /></QueryClientProvider></ThemeProvider>)
  return { client, router, view, user: userEvent.setup() }
}
beforeEach(() => {
  invoke.mockReset()
  localStorage.clear()
})
describe('shell and shared pages', () => {
  it('renders real home summaries and shared navigation', async () => {
    invoke.mockResolvedValue([task(), task({ id: 'done', status: 'completed', fileName: 'done.zip' }), task({ id: 'bad', status: 'failed', fileName: 'bad.zip', error: { code: 'NETWORK_TIMEOUT', message: 'timed out', retryable: true } })])
    const { user, client } = await setup()
    expect(await screen.findByText('Active transfers')).toBeInTheDocument()
    expect(screen.getByText('Needs attention')).toBeInTheDocument()
    expect(screen.getAllByText('bad.zip')).toHaveLength(2)
    await user.click(screen.getByRole('link', { name: 'View downloads' }))
    expect(await screen.findByRole('heading', { name: 'Downloads' })).toBeInTheDocument()
    expect(screen.getByText('3 files')).toBeInTheDocument()
    expect(screen.getAllByRole('navigation')[0]).toBeInTheDocument()
    expect(client.getQueryData(downloadsKey)).toHaveLength(3)
  })
  it('keeps global Add usable on empty Downloads and Settings', async () => {
    invoke.mockResolvedValue([])
    const { user } = await setup('/downloads')
    await screen.findByText('Your next download starts here')
    await user.click(screen.getByRole('button', { name: 'Add your first download' }))
    expect(await screen.findByRole('dialog')).toBeInTheDocument()
    await user.keyboard('{Escape}')
    await user.click(screen.getAllByRole('link', { name: 'Settings' })[0])
    await screen.findByRole('heading', { name: 'Settings' })
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    expect(await screen.findByRole('dialog')).toBeInTheDocument()
  })
  it('renders initial query failure instead of a router crash', async () => {
    invoke.mockRejectedValue({ code: 'INTERNAL', message: 'unavailable', retryable: false })
    await setup('/downloads')
    expect(await screen.findByText('Could not load downloads', {}, { timeout: 4000 })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument()
    expect(screen.queryByText('This page could not be opened')).not.toBeInTheDocument()
  })
  it('updates the visible table from the shared cache', async () => {
    invoke.mockResolvedValue([task()])
    const { client } = await setup('/downloads')
    await screen.findByRole('table')
    mergeTask(client, task({ status: 'paused', revision: 2 }))
    await waitFor(() => expect(within(screen.getByRole('table')).getByText('Paused')).toBeInTheDocument())
  })
})
