import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, expect, it, vi } from 'vitest'
import type { QueuePolicy } from './api'
import { QueuesPage } from './queues-page'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
const queue = {
  id: 'default',
  name: 'Main queue',
  paused: false,
  maxConcurrent: 3,
  bytesPerSecond: 0,
  taskCount: 4,
}
beforeEach(() => {
  invoke.mockReset()
  invoke.mockImplementation(
    async (command: string, args: { input: QueuePolicy }) => {
      if (command === 'get_queue_policies') return [queue]
      if (command === 'save_queue_policy')
        return { ...args.input, id: args.input.id || 'new' }
      throw new Error(`Unexpected command ${command}`)
    },
  )
})
function setup() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  render(
    <QueryClientProvider client={client}>
      <QueuesPage />
    </QueryClientProvider>,
  )
  return userEvent.setup()
}
it('shows queue policies without loading or displaying downloads', async () => {
  setup()
  expect(
    await screen.findByRole('heading', { name: 'Main queue' }),
  ).toBeInTheDocument()
  expect(screen.getByText('Unlimited')).toBeInTheDocument()
  expect(screen.queryByRole('table')).not.toBeInTheDocument()
  expect(
    invoke.mock.calls.every(([command]) => command === 'get_queue_policies'),
  ).toBe(true)
})
it('creates a named, bandwidth-limited paused queue and closes the dialog', async () => {
  const user = setup()
  await user.click(screen.getByRole('button', { name: 'Create queue' }))
  await user.type(screen.getByLabelText('Queue name'), '  Overnight  ')
  await user.click(screen.getByLabelText('Limit bandwidth'))
  await user.clear(screen.getByLabelText('Bandwidth limit'))
  await user.type(screen.getByLabelText('Bandwidth limit'), '0.5')
  await user.click(screen.getByLabelText('Create as paused'))
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', {
      name: 'Create queue',
    }),
  )
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('save_queue_policy', {
      input: {
        id: '',
        name: 'Overnight',
        paused: true,
        maxConcurrent: 3,
        bytesPerSecond: 500000,
        taskCount: 0,
      },
    }),
  )
  expect(await screen.findByRole('status')).toHaveTextContent(
    'Overnight created.',
  )
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
})
it('keeps edits available after a failed save', async () => {
  const user = setup()
  await user.click(
    await screen.findByRole('button', { name: 'Edit Main queue' }),
  )
  invoke.mockRejectedValueOnce(new Error('Save failed'))
  await user.click(screen.getByRole('button', { name: 'Save changes' }))
  expect(await screen.findByRole('alert')).toHaveTextContent('Save failed')
  expect(screen.getByLabelText('Queue name')).toHaveValue('Main queue')
})

it('preserves the bandwidth limit when changing units', async () => {
  const user = setup()
  await user.click(screen.getByRole('button', { name: 'Create queue' }))
  await user.type(screen.getByLabelText('Queue name'), 'Media')
  await user.click(screen.getByLabelText('Limit bandwidth'))
  expect(screen.getByLabelText('Bandwidth unit')).toHaveValue('MB/s')
  await user.selectOptions(screen.getByLabelText('Bandwidth unit'), 'GB/s')
  expect(screen.getByLabelText('Bandwidth limit')).toHaveValue(0.001)
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', {
      name: 'Create queue',
    }),
  )
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('save_queue_policy', {
      input: {
        id: '',
        name: 'Media',
        paused: false,
        maxConcurrent: 3,
        bytesPerSecond: 1_000_000,
        taskCount: 0,
      },
    }),
  )
})
it('displays large limits in GB/s and preserves existing byte values when editing', async () => {
  const existing = { ...queue, bytesPerSecond: 1_000_000_000 }
  invoke.mockImplementation(
    async (command: string, args: { input: QueuePolicy }) =>
      command === 'get_queue_policies' ? [existing] : args.input,
  )
  const user = setup()
  expect(await screen.findByText('GB/s')).toBeInTheDocument()
  await user.click(screen.getByRole('button', { name: 'Edit Main queue' }))
  expect(screen.getByLabelText('Bandwidth unit')).toHaveValue('GB/s')
  expect(screen.getByLabelText('Bandwidth limit')).toHaveValue(1)
  await user.click(screen.getByRole('button', { name: 'Save changes' }))
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('save_queue_policy', {
      input: existing,
    }),
  )
})

it('pauses and resumes a queue without changing its limits', async () => {
  let saved = { ...queue }
  invoke.mockImplementation(
    async (command: string, args: { input: QueuePolicy }) => {
      if (command === 'get_queue_policies') return [saved]
      saved = args.input
      return saved
    },
  )
  const user = setup()
  await user.click(
    await screen.findByRole('button', { name: 'Pause Main queue' }),
  )
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('save_queue_policy', {
      input: { ...queue, paused: true },
    }),
  )
  const resume = await screen.findByRole('button', {
    name: 'Resume Main queue',
  })
  await waitFor(() => expect(resume).toBeEnabled())
  expect(screen.getByRole('status')).toHaveTextContent(
    'Transfers are stopped until you resume.',
  )
  await user.click(resume)
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('save_queue_policy', { input: queue }),
  )
  expect(
    await screen.findByRole('button', { name: 'Pause Main queue' }),
  ).toBeInTheDocument()
})
it('keeps the queue unchanged when a pause fails and allows retry', async () => {
  const user = setup()
  const pause = await screen.findByRole('button', { name: 'Pause Main queue' })
  invoke.mockRejectedValueOnce(new Error('Could not pause queue'))
  await user.click(pause)
  expect(await screen.findByRole('alert')).toHaveTextContent(
    'Could not pause queue',
  )
  expect(pause).toBeEnabled()
  expect(
    screen.queryByRole('button', { name: 'Resume Main queue' }),
  ).not.toBeInTheDocument()
})
it('duplicates settings into a reviewable new queue without copying membership', async () => {
  const existing = {
    ...queue,
    paused: true,
    maxConcurrent: 5,
    bytesPerSecond: 2_000_000,
  }
  invoke.mockImplementation(
    async (command: string, args: { input: QueuePolicy }) =>
      command === 'get_queue_policies'
        ? [existing]
        : { ...args.input, id: 'copy' },
  )
  const user = setup()
  await user.click(
    await screen.findByRole('button', { name: 'Duplicate Main queue' }),
  )
  expect(screen.getByLabelText('Queue name')).toHaveValue('Main queue (copy)')
  expect(screen.getByLabelText('Simultaneous transfers')).toHaveValue(5)
  expect(screen.getByLabelText('Bandwidth limit')).toHaveValue(2)
  expect(screen.getByLabelText('Create as paused')).toBeChecked()
  expect(
    invoke.mock.calls.some(([command]) => command === 'save_queue_policy'),
  ).toBe(false)
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', {
      name: 'Create queue',
    }),
  )
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('save_queue_policy', {
      input: { ...existing, id: '', name: 'Main queue (copy)', taskCount: 0 },
    }),
  )
})

it.each([false, true])(
  'hides pause/resume for an empty queue (paused: %s)',
  async (paused) => {
    invoke.mockResolvedValue([{ ...queue, paused, taskCount: 0 }])
    setup()
    await screen.findByRole('heading', { name: 'Main queue' })
    expect(
      screen.queryByRole('button', { name: 'Pause Main queue' }),
    ).not.toBeInTheDocument()
    expect(
      screen.queryByRole('button', { name: 'Resume Main queue' }),
    ).not.toBeInTheDocument()
    expect(
      screen.getByRole('button', { name: 'Edit Main queue' }),
    ).toBeEnabled()
  },
)
