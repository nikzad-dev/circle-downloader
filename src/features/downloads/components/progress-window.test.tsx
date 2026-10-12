import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, expect, it, vi } from 'vitest'
import { task } from '@/test/fixtures'
import { downloadsKey, mergeTask } from '../api/downloads'
import { ProgressContext } from '../hooks/progress-context'
import { ProgressWindow, ProgressWindowHost } from './progress-window'

const { invoke, receive, subscriptions } = vi.hoisted(() => ({
  invoke: vi.fn(),
  receive: {
    visibility: undefined as
      undefined | ((event: { payload: boolean }) => void),
  },
  subscriptions: new Set<number>(),
}))
vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class {
    onmessage = (_payload: unknown) => {}
  },
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(
    async (_name: string, callback: NonNullable<typeof receive.visibility>) => {
      receive.visibility = callback
      return vi.fn()
    },
  ),
}))
beforeEach(() => {
  invoke.mockReset()
  subscriptions.clear()
  invoke.mockImplementation(
    async (command: string, args?: { subscriptionId?: number }) => {
      if (command === 'list_downloads') return [task()]
      if (command === 'progress_window_ready') return true
      if (command === 'subscribe_progress') {
        subscriptions.add(1)
        return 1
      }
      if (command === 'unsubscribe_progress') {
        subscriptions.delete(args!.subscriptionId!)
        return
      }
      if (command === 'close_progress_window') return
      throw new Error(command)
    },
  )
})
function setup(tasks = [task()], queuePaused = false) {
  invoke.mockImplementation(async (command: string) =>
    command === 'list_downloads'
      ? tasks
      : command === 'get_queue_policies'
        ? [
            {
              id: 'default',
              name: 'Default',
              paused: queuePaused,
              maxConcurrent: 3,
              bytesPerSecond: 0,
              taskCount: tasks.length,
            },
          ]
        : undefined,
  )
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  client.setQueryData(downloadsKey, tasks)
  const view = render(
    <QueryClientProvider client={client}>
      <ProgressWindow />
    </QueryClientProvider>,
  )
  return { client, view, user: userEvent.setup() }
}
it('shows live bytes, speed and time left and retains a watched transfer on completion', async () => {
  const { client } = setup()
  await screen.findByRole('article', { name: 'file.zip' })
  expect(
    screen.getByRole('progressbar', { name: 'Progress for file.zip' }),
  ).toHaveAttribute('aria-valuenow', '10')
  expect(screen.getByText('45s')).toBeInTheDocument()
  act(() =>
    mergeTask(
      client,
      task({ status: 'completed', downloadedBytes: 1000, revision: 2 }),
    ),
  )
  await waitFor(() =>
    expect(screen.getByRole('progressbar')).toHaveAttribute(
      'aria-valuenow',
      '100',
    ),
  )
  expect(
    within(screen.getByRole('article')).getByText('Completed'),
  ).toBeInTheDocument()
  expect(
    screen.getByRole('button', { name: 'Open file.zip' }),
  ).toBeInTheDocument()
})
it('handles unknown sizes, paused transfers, verification, failures and completed history', async () => {
  const { user } = setup([
    task({ id: 'unknown', fileName: 'unknown.bin', totalBytes: null }),
    task({ id: 'paused', fileName: 'paused.zip', status: 'paused' }),
    task({
      id: 'verify',
      fileName: 'verify.zip',
      status: 'verifying',
      downloadedBytes: 1000,
    }),
    task({
      id: 'failed',
      fileName: 'failed.zip',
      status: 'failed',
      error: { code: 'NETWORK_TIMEOUT', message: 'timeout', retryable: true },
    }),
    task({ id: 'done', fileName: 'done.zip', status: 'completed' }),
  ])
  expect(
    screen.getByRole('progressbar', { name: 'Progress for unknown.bin' }),
  ).not.toHaveAttribute('aria-valuenow')
  expect(screen.getByText('Checking file integrity')).toBeInTheDocument()
  expect(
    screen.getByRole('button', { name: 'Resume paused.zip' }),
  ).toBeInTheDocument()
  expect(screen.getByText(/The connection timed out/)).toBeInTheDocument()
  expect(
    screen.queryByRole('article', { name: 'done.zip' }),
  ).not.toBeInTheDocument()
  await user.click(screen.getByRole('button', { name: 'Completed' }))
  expect(screen.getByRole('article', { name: 'done.zip' })).toBeInTheDocument()
})
it('closes with Escape without pausing or canceling transfers', () => {
  setup()
  fireEvent.keyDown(window, { key: 'Escape' })
  expect(invoke).toHaveBeenCalledWith('close_progress_window')
  expect(
    invoke.mock.calls.some(
      ([command]) =>
        command === 'pause_download' || command === 'cancel_download',
    ),
  ).toBe(false)
})
it('releases the progress subscription when hidden and reconnects on reopen', async () => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  const view = render(
    <QueryClientProvider client={client}>
      <ProgressWindowHost />
    </QueryClientProvider>,
  )
  await screen.findByRole('heading', { name: 'Download progress' })
  await waitFor(() => expect(subscriptions.size).toBe(1))
  act(() => receive.visibility?.({ payload: false }))
  await waitFor(() => expect(subscriptions.size).toBe(0))
  expect(
    screen.queryByRole('heading', { name: 'Download progress' }),
  ).not.toBeInTheDocument()
  act(() => receive.visibility?.({ payload: true }))
  await screen.findByRole('heading', { name: 'Download progress' })
  await waitFor(() => expect(subscriptions.size).toBe(1))
  view.unmount()
  await waitFor(() => expect(subscriptions.size).toBe(0))
  client.clear()
})
it('provides a reconnect action when live events are unavailable', async () => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  const reconnect = vi.fn()
  render(
    <QueryClientProvider client={client}>
      <ProgressContext value={{ unavailable: true, reconnect }}>
        <ProgressWindow />
      </ProgressContext>
    </QueryClientProvider>,
  )
  expect(
    screen.getByText(/Checking progress every 2 seconds/),
  ).toBeInTheDocument()
  await userEvent.click(screen.getByRole('button', { name: 'Reconnect' }))
  expect(reconnect).toHaveBeenCalledOnce()
})

it('explains paused queues and resumes them without changing queue limits', async () => {
  const { user } = setup(
    [
      task({
        status: 'queued',
        totalBytes: null,
        downloadedBytes: 0,
        queueId: 'default',
      }),
    ],
    true,
  )
  expect(await screen.findByText('Queue paused')).toBeInTheDocument()
  const card = within(screen.getByRole('article'))
  expect(card.getByText('0 B downloaded · Size pending')).toBeInTheDocument()
  expect(card.queryByText('Unknown size')).not.toBeInTheDocument()
  expect(card.queryByText('Time left')).not.toBeInTheDocument()
  expect(card.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0')
  await user.click(card.getByRole('button', { name: 'Resume queue' }))
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('save_queue_policy', {
      input: {
        id: 'default',
        name: 'Default',
        paused: false,
        maxConcurrent: 3,
        bytesPerSecond: 0,
        taskCount: 1,
      },
    }),
  )
})
