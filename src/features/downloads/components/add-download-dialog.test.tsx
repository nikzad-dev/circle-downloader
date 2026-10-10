import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useRef, useState } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { task } from '@/test/fixtures'
import { downloadsKey } from '../api/downloads'
import { AddDownloadDialog } from './add-download-dialog'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
function Harness() {
  const [open, setOpen] = useState(false)
  const triggerRef = useRef<HTMLButtonElement>(null)
  return (
    <>
      <button ref={triggerRef} onClick={() => setOpen(true)}>Open add</button>
      <AddDownloadDialog open={open} onOpenChange={setOpen} triggerRef={triggerRef} />
    </>
  )
}
function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  render(<QueryClientProvider client={client}><Harness /></QueryClientProvider>)
  return { client, user: userEvent.setup() }
}
function stubSupportCommands() {
  invoke.mockImplementation(async (command: string) => {
    if (command === 'list_destinations') {
      return [{ id: 'default', label: 'App-managed downloads', displayPath: '/tmp/downloads', isDefault: true }]
    }
    if (command === 'get_settings') {
      return { maxConcurrentDownloads: 3, defaultDestinationId: 'default', autoStart: true, maxConnectionsPerDownload: 1 }
    }
    throw new Error(`Unexpected command ${command}`)
  })
}
beforeEach(() => {
  invoke.mockReset()
  stubSupportCommands()
})
describe('add Download workflow', () => {
  it('validates input without invoking native commands', async () => {
    const { user } = setup()
    await user.click(screen.getByText('Open add'))
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    expect(await screen.findByText('Enter a valid URL.')).toBeInTheDocument()
    await user.type(screen.getByLabelText('Download URL'), 'http://example.com/file')
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    expect(await screen.findByText('Use HTTPS, or HTTP for localhost only.')).toBeInTheDocument()
    expect(invoke.mock.calls.every(([command]) => command !== 'create_download')).toBe(true)
  })
  it('creates trimmed input, omits blank filename, writes shared cache and restores focus', async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === 'list_destinations') {
        return [{ id: 'default', label: 'App-managed downloads', displayPath: '/tmp/downloads', isDefault: true }]
      }
      if (command === 'get_settings') {
        return { maxConcurrentDownloads: 3, defaultDestinationId: 'default', autoStart: true, maxConnectionsPerDownload: 1 }
      }
      if (command === 'create_download')
        return task({ status: 'paused' })
      throw new Error(command)
    })
    const { user, client } = setup()
    await user.click(screen.getByText('Open add'))
    await waitFor(() => expect(screen.getByLabelText('Download URL')).toHaveFocus())
    await user.type(screen.getByLabelText('Download URL'), ' https://example.com/file.zip ')
    await user.click(screen.getByRole('checkbox', { name: 'Start now' }))
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(invoke).toHaveBeenCalledWith('create_download', { input: { url: 'https://example.com/file.zip', destinationId: 'default', startNow: false, requestId: expect.any(String) as string } })
    expect(client.getQueryData(downloadsKey)).toEqual([task({ status: 'paused' })])
    await waitFor(() => expect(screen.getByText('Open add')).toHaveFocus())
  })
  it('retains values and the request ID when retrying a failed operation', async () => {
    let createCalls = 0
    invoke.mockImplementation(async (command: string) => {
      if (command === 'list_destinations') {
        return [{ id: 'default', label: 'App-managed downloads', displayPath: '/tmp/downloads', isDefault: true }]
      }
      if (command === 'get_settings') {
        return { maxConcurrentDownloads: 3, defaultDestinationId: 'default', autoStart: true, maxConnectionsPerDownload: 1 }
      }
      if (command === 'create_download') {
        createCalls += 1
        if (createCalls === 1)
          throw Object.assign(new Error('full'), { code: 'DISK_FULL', retryable: false })
        return task()
      }
      throw new Error(command)
    })
    const { user } = setup()
    await user.click(screen.getByText('Open add'))
    await user.type(screen.getByLabelText('Download URL'), 'https://example.com/file.zip')
    await user.type(screen.getByLabelText(/Filename/), ' custom.zip ')
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Free some disk space')
    expect(screen.getByLabelText('Download URL')).toHaveValue('https://example.com/file.zip')
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    await waitFor(() => expect(createCalls).toBe(2))
    const createArgs = invoke.mock.calls.filter(([command]) => command === 'create_download').map(([, input]): unknown => input)
    expect(createArgs[0]).toEqual(createArgs[1])
    expect(createArgs[1]).toMatchObject({ input: { suggestedName: 'custom.zip' } })
  })
  it('prevents duplicate submissions while pending', async () => {
    let resolve!: (value: ReturnType<typeof task>) => void
    invoke.mockImplementation(async (command: string) => {
      if (command === 'list_destinations') {
        return [{ id: 'default', label: 'App-managed downloads', displayPath: '/tmp/downloads', isDefault: true }]
      }
      if (command === 'get_settings') {
        return { maxConcurrentDownloads: 3, defaultDestinationId: 'default', autoStart: true, maxConnectionsPerDownload: 1 }
      }
      if (command === 'create_download') {
        return new Promise((done) => {
          resolve = done
        })
      }
      throw new Error(command)
    })
    const { user } = setup()
    await user.click(screen.getByText('Open add'))
    await user.type(screen.getByLabelText('Download URL'), 'https://example.com/file.zip')
    await user.dblClick(screen.getByRole('button', { name: 'Add download' }))
    expect(invoke.mock.calls.filter(([command]) => command === 'create_download')).toHaveLength(1)
    expect(screen.getByRole('button', { name: 'Adding…' })).toBeDisabled()
    await user.keyboard('{Escape}')
    expect(screen.getByRole('dialog')).toBeInTheDocument()
    resolve(task())
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  })
  it('validates a supplied SHA-256 and passes it to the native command', async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === 'list_destinations') {
        return [{ id: 'default', label: 'App-managed downloads', displayPath: '/tmp/downloads', isDefault: true }]
      }
      if (command === 'get_settings') {
        return { maxConcurrentDownloads: 3, defaultDestinationId: 'default', autoStart: true, maxConnectionsPerDownload: 1 }
      }
      if (command === 'create_download')
        return task()
      throw new Error(command)
    })
    const { user } = setup()
    await user.click(screen.getByText('Open add'))
    await user.type(screen.getByLabelText('Download URL'), 'https://example.com/file.zip')
    await user.type(screen.getByLabelText(/SHA-256/), 'bad')
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    expect(await screen.findByText('Enter a 64-character SHA-256 checksum.')).toBeInTheDocument()
    expect(invoke.mock.calls.every(([command]) => command !== 'create_download')).toBe(true)
    await user.clear(screen.getByLabelText(/SHA-256/))
    const checksum = 'a'.repeat(64)
    await user.type(screen.getByLabelText(/SHA-256/), checksum)
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    await waitFor(() => expect(invoke.mock.calls.some(([command]) => command === 'create_download')).toBe(true))
    const createCall = invoke.mock.calls.find(([command]) => command === 'create_download')
    expect(createCall?.[1]).toMatchObject({ input: { expectedSha256: checksum } })
  })
  it('closes with Escape and restores trigger focus', async () => {
    const { user } = setup()
    await user.click(screen.getByText('Open add'))
    await user.keyboard('{Escape}')
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(screen.getByText('Open add')).toHaveFocus()
  })
})
