import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { task } from '@/test/fixtures'
import {
  AddDownloadWindow,
  DownloadWindowHost,
  NewDownloadForm,
} from './add-download-window'

const invoke = vi.hoisted(() => vi.fn())
const events = vi.hoisted(() => ({
  receive: undefined as
    | undefined
    | ((event: {
        payload: {
          initialUrl: string
          initialUrls?: string[]
          bulk: boolean
          generation: number
          requiresConfirmation?: boolean
        }
      }) => void),
  unlisten: vi.fn(),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(
    async (_name: string, callback: NonNullable<typeof events.receive>) => {
      events.receive = callback
      return events.unlisten
    },
  ),
}))
function setup() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  const done = vi.fn()
  render(
    <QueryClientProvider client={client}>
      <NewDownloadForm onDone={done} />
    </QueryClientProvider>,
  )
  return { user: userEvent.setup(), done }
}
function support(command: string) {
  if (command === 'open_progress_window') return undefined
  if (command === 'list_destinations')
    return [
      {
        id: 'default',
        label: 'Downloads',
        displayPath: '/tmp/downloads',
        isDefault: true,
      },
    ]
  if (command === 'get_settings')
    return {
      maxConcurrentDownloads: 3,
      defaultDestinationId: 'default',
      autoStart: true,
      maxConnectionsPerDownload: 4,
    }
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
  throw new Error(command)
}
beforeEach(() => {
  invoke.mockReset()
  invoke.mockImplementation(async (command: string) =>
    command === 'create_configured_download' ? task() : support(command),
  )
})
describe('compact download window', () => {
  it('exposes and resumes the paused default queue even with organization hidden', async () => {
    let paused = true
    const queue = () => ({
      id: 'default',
      name: 'Downloads',
      paused,
      maxConcurrent: 3,
      bytesPerSecond: 0,
      taskCount: 1,
    })
    invoke.mockImplementation(async (command: string, args: unknown) => {
      if (command === 'get_queue_policies') return [queue()]
      if (command === 'save_queue_policy') {
        expect(args).toEqual({ input: { ...queue(), paused: false } })
        paused = false
        return queue()
      }
      if (command === 'create_configured_download') return task()
      return support(command)
    })
    const { user, done } = setup()
    expect(await screen.findByText('Downloads queue is paused')).toBeVisible()
    await user.click(screen.getByRole('button', { name: 'Resume queue' }))
    await waitFor(() =>
      expect(
        screen.queryByText('Downloads queue is paused'),
      ).not.toBeInTheDocument(),
    )
    await user.type(
      screen.getByLabelText('Download URL'),
      'https://example.com/file.zip',
    )
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    await waitFor(() => expect(done).toHaveBeenCalled())
    expect(
      invoke.mock.calls.find(
        ([command]) => command === 'create_configured_download',
      )?.[1],
    ).toMatchObject({ input: { startNow: true } })
  })

  it('keeps the queue pause visible if resuming fails', async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === 'get_queue_policies')
        return [
          {
            id: 'default',
            name: 'Downloads',
            paused: true,
            maxConcurrent: 3,
            bytesPerSecond: 0,
            taskCount: 0,
          },
        ]
      if (command === 'save_queue_policy') throw new Error('Resume failed')
      return support(command)
    })
    const { user } = setup()
    await user.click(
      await screen.findByRole('button', { name: 'Resume queue' }),
    )
    expect(await screen.findByRole('alert')).toBeVisible()
    expect(screen.getByText('Downloads queue is paused')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Resume queue' })).toBeEnabled()
  })

  it('keeps optional fields hidden until enabled and validates URLs', async () => {
    const { user } = setup()
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(screen.queryByLabelText('Filename')).not.toBeInTheDocument()
    expect(screen.queryByLabelText('SHA-256')).not.toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'Category' })).toHaveValue('')
    await waitFor(() =>
      expect(
        screen.getByRole('button', { name: 'Add download' }),
      ).toBeEnabled(),
    )
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    expect(await screen.findByText('Enter a valid URL.')).toBeInTheDocument()
    expect(
      invoke.mock.calls.some(
        ([command]) => command === 'create_configured_download',
      ),
    ).toBe(false)
  })
  it('submits basic defaults with acceleration explicitly disabled and closes only on success', async () => {
    const { user, done } = setup()
    await user.type(
      screen.getByLabelText('Download URL'),
      ' https://example.com/file.zip ',
    )
    await user.click(screen.getByLabelText('Start automatically'))
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    await waitFor(() => expect(done).toHaveBeenCalledOnce())
    expect(
      invoke.mock.calls.some(([command]) => command === 'open_progress_window'),
    ).toBe(false)
    expect(invoke).toHaveBeenCalledWith('create_configured_download', {
      input: {
        url: 'https://example.com/file.zip',
        destinationId: 'default',
        startNow: false,
        requestId: expect.any(String) as string,
      },
      options: expect.objectContaining({
        queueId: 'default',
        connections: 1,
        schedule: null,
        credentialId: null,
      }) as unknown,
    })
  })
  it('retains input and request ID on failed retries', async () => {
    let calls = 0
    invoke.mockImplementation(async (command: string) => {
      if (command !== 'create_configured_download') return support(command)
      if (++calls === 1)
        throw Object.assign(new Error('full'), {
          code: 'DISK_FULL',
          retryable: false,
        })
      return task()
    })
    const { user, done } = setup()
    await user.type(
      screen.getByLabelText('Download URL'),
      'https://example.com/file.zip',
    )
    await user.click(screen.getByText('More options', { exact: false }))
    await user.click(screen.getByLabelText('Custom filename'))
    await user.type(screen.getByLabelText('Filename'), ' custom.zip ')
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Free some disk space',
    )
    expect(done).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    await waitFor(() => expect(done).toHaveBeenCalledOnce())
    const requests = invoke.mock.calls.filter(
      ([command]) => command === 'create_configured_download',
    )
    expect(requests[0]?.[1]).toEqual(requests[1]?.[1])
  })
  it('enables checksum, organization and per-download connections', async () => {
    const { user } = setup()
    await user.type(
      screen.getByLabelText('Download URL'),
      'https://example.com/file.zip',
    )
    await user.click(screen.getByText('More options', { exact: false }))
    await user.click(screen.getByLabelText('Verify SHA-256'))
    await user.type(screen.getByLabelText('SHA-256'), 'bad')
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    expect(
      await screen.findByText('Enter a 64-character SHA-256 checksum.'),
    ).toBeInTheDocument()
    await user.clear(screen.getByLabelText('SHA-256'))
    await user.type(screen.getByLabelText('SHA-256'), 'a'.repeat(64))
    await user.click(screen.getByLabelText(/Organize download/))
    await user.selectOptions(
      screen.getByRole('combobox', { name: 'Category' }),
      'Documents',
    )
    await user.click(screen.getByLabelText('Multiple connections'))
    await user.selectOptions(screen.getByLabelText('Connections'), '2')
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    await waitFor(() =>
      expect(
        invoke.mock.calls.some(
          ([command]) => command === 'create_configured_download',
        ),
      ).toBe(true),
    )
    const args: unknown = invoke.mock.calls.find(
      ([command]) => command === 'create_configured_download',
    )?.[1]
    expect(args).toMatchObject({
      input: { expectedSha256: 'a'.repeat(64) },
      options: { category: 'Documents', connections: 2 },
    })
  })
})

it('preserves both drafts when switching modes with the keyboard', async () => {
  const { AddDownloadWindow } = await import('./add-download-window')
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  render(
    <QueryClientProvider client={client}>
      <AddDownloadWindow />
    </QueryClientProvider>,
  )
  const user = userEvent.setup()
  await user.type(
    screen.getByLabelText('Download URL'),
    'https://example.com/single',
  )
  screen.getByRole('tab', { name: 'Single URL' }).focus()
  await user.keyboard('{ArrowRight}')
  expect(screen.getByRole('tab', { name: 'Multiple URLs' })).toHaveFocus()
  await user.type(
    screen.getByLabelText('Download URLs'),
    'https://example.com/bulk',
  )
  await user.click(screen.getByRole('tab', { name: 'Single URL' }))
  expect(screen.getByLabelText('Download URL')).toHaveValue(
    'https://example.com/single',
  )
  await user.click(screen.getByRole('tab', { name: 'Multiple URLs' }))
  expect(screen.getByLabelText('Download URLs')).toHaveValue(
    'https://example.com/bulk',
  )
})

it('reuses the window and resets drafts for each new request, including bulk mode', async () => {
  invoke.mockImplementation(async (command: string) =>
    command === 'download_window_ready'
      ? { initialUrl: 'https://example.com/first', bulk: false, generation: 1 }
      : support(command),
  )
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  const { unmount } = render(
    <QueryClientProvider client={client}>
      <DownloadWindowHost />
    </QueryClientProvider>,
  )
  await waitFor(() =>
    expect(screen.getByLabelText('Download URL')).toHaveValue(
      'https://example.com/first',
    ),
  )
  const user = userEvent.setup()
  await user.click(screen.getByRole('checkbox', { name: /Organize download/ }))
  await user.selectOptions(
    screen.getByRole('combobox', { name: 'Category' }),
    'Videos',
  )
  await act(async () =>
    events.receive?.({
      payload: {
        initialUrl: 'https://example.com/second',
        bulk: false,
        generation: 2,
      },
    }),
  )
  expect(screen.getByLabelText('Download URL')).toHaveValue(
    'https://example.com/second',
  )
  expect(
    screen.getByRole('checkbox', { name: /Organize download/ }),
  ).not.toBeChecked()
  await act(async () =>
    events.receive?.({
      payload: { initialUrl: '', bulk: true, generation: 3 },
    }),
  )
  expect(screen.getByRole('tab', { name: 'Multiple URLs' })).toHaveAttribute(
    'aria-selected',
    'true',
  )
  expect(screen.getByLabelText('Download URLs')).toHaveValue('')
  // A delayed ready response/event must not restore an older draft.
  await act(async () =>
    events.receive?.({
      payload: {
        initialUrl: 'https://example.com/stale',
        bulk: false,
        generation: 1,
      },
    }),
  )
  expect(screen.getByRole('tab', { name: 'Multiple URLs' })).toHaveAttribute(
    'aria-selected',
    'true',
  )
  unmount()
  expect(events.unlisten).toHaveBeenCalled()
})

it('previews automatic file categories and retains a manual selection when the URL changes', async () => {
  const { user } = setup()
  await user.type(
    screen.getByLabelText('Download URL'),
    'https://example.com/report.PDF?name=movie.mp4',
  )
  expect(
    screen.getByRole('option', { name: 'Automatic (Documents)' }),
  ).toBeInTheDocument()
  await user.selectOptions(
    screen.getByRole('combobox', { name: 'Category' }),
    'Videos',
  )
  await user.clear(screen.getByLabelText('Download URL'))
  await user.type(
    screen.getByLabelText('Download URL'),
    'https://example.com/song.mp3',
  )
  expect(screen.getByRole('combobox', { name: 'Category' })).toHaveValue(
    'Videos',
  )
  await user.click(screen.getByRole('button', { name: 'Add download' }))
  await waitFor(() =>
    expect(
      invoke.mock.calls.find(
        ([command]) => command === 'create_configured_download',
      )?.[1],
    ).toMatchObject({ options: { category: 'Videos' } }),
  )
})

it('fills bulk paste requests and protects a visible draft until replacement is chosen', async () => {
  invoke.mockImplementation(async (command: string) =>
    command === 'download_window_ready'
      ? { initialUrl: 'https://example.com/draft', bulk: false, generation: 1 }
      : support(command),
  )
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  render(
    <QueryClientProvider client={client}>
      <DownloadWindowHost />
    </QueryClientProvider>,
  )
  await waitFor(() =>
    expect(screen.getByLabelText('Download URL')).toHaveValue(
      'https://example.com/draft',
    ),
  )
  await act(async () =>
    events.receive?.({
      payload: {
        initialUrl: '',
        initialUrls: ['https://example.com/a.mp3', 'https://example.com/b.pdf'],
        bulk: true,
        generation: 2,
        requiresConfirmation: true,
      },
    }),
  )
  expect(screen.getByLabelText('Download URL')).toHaveValue(
    'https://example.com/draft',
  )
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: 'Use new links' }))
  expect(screen.getByRole('tab', { name: 'Multiple URLs' })).toHaveAttribute(
    'aria-selected',
    'true',
  )
  expect(screen.getByLabelText('Download URLs')).toHaveValue(
    'https://example.com/a.mp3\nhttps://example.com/b.pdf',
  )
  await act(async () =>
    events.receive?.({
      payload: {
        initialUrl: 'https://example.com/other',
        bulk: false,
        generation: 3,
        requiresConfirmation: true,
      },
    }),
  )
  await user.click(screen.getByRole('button', { name: 'Keep draft' }))
  expect(screen.getByLabelText('Download URLs')).toHaveValue(
    'https://example.com/a.mp3\nhttps://example.com/b.pdf',
  )
})

it.each([false, true])(
  'closes the download window with Escape in bulk mode %s',
  async (initialBulk) => {
    invoke.mockImplementation(async (command: string) =>
      command === 'close_download_window' ? undefined : support(command),
    )
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    })
    const { unmount } = render(
      <QueryClientProvider client={client}>
        <AddDownloadWindow initialBulk={initialBulk} />
      </QueryClientProvider>,
    )
    fireEvent.keyDown(window, { key: 'Escape' })
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('close_download_window'),
    )
    const closes = invoke.mock.calls.filter(
      ([command]) => command === 'close_download_window',
    ).length
    unmount()
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(
      invoke.mock.calls.filter(
        ([command]) => command === 'close_download_window',
      ),
    ).toHaveLength(closes)
  },
)
it('leaves Escape handled by nested controls alone and ignores key repeats', () => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  render(
    <QueryClientProvider client={client}>
      <AddDownloadWindow />
    </QueryClientProvider>,
  )
  const event = new KeyboardEvent('keydown', {
    key: 'Escape',
    bubbles: true,
    cancelable: true,
  })
  event.preventDefault()
  fireEvent(screen.getByLabelText('Download URL'), event)
  fireEvent.keyDown(window, { key: 'Escape', repeat: true })
  expect(
    invoke.mock.calls.some(([command]) => command === 'close_download_window'),
  ).toBe(false)
})
it('does not close on Escape while a download is being submitted', async () => {
  let complete!: (value: ReturnType<typeof task>) => void
  invoke.mockImplementation(async (command: string) =>
    command === 'create_configured_download'
      ? new Promise<ReturnType<typeof task>>((resolve) => {
          complete = resolve
        })
      : command === 'close_download_window'
        ? undefined
        : support(command),
  )
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  render(
    <QueryClientProvider client={client}>
      <AddDownloadWindow initialUrl="https://example.com/file.zip" />
    </QueryClientProvider>,
  )
  const user = userEvent.setup()
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Add download' })).toBeEnabled(),
  )
  await user.click(screen.getByRole('button', { name: 'Add download' }))
  await screen.findByRole('button', { name: 'Adding…' })
  fireEvent.keyDown(window, { key: 'Escape' })
  expect(
    invoke.mock.calls.some(([command]) => command === 'close_download_window'),
  ).toBe(false)
  await act(async () => complete(task()))
})

it('opens progress automatically only after successfully adding an immediately started download', async () => {
  const { user, done } = setup()
  await user.type(
    screen.getByLabelText('Download URL'),
    'https://example.com/file.zip',
  )
  await user.click(screen.getByRole('button', { name: 'Add download' }))
  await waitFor(() => expect(done).toHaveBeenCalledOnce())
  expect(invoke).toHaveBeenCalledWith('open_progress_window')
})
