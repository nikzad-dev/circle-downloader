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
beforeEach(() => {
  invoke.mockReset()
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
    expect(invoke).not.toHaveBeenCalled()
  })
  it('creates trimmed input, omits blank filename, writes shared cache and restores focus', async () => {
    invoke.mockResolvedValue(task({ status: 'paused' }))
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
    invoke.mockRejectedValueOnce({ code: 'DISK_FULL', message: 'full', retryable: false }).mockResolvedValueOnce(task())
    const { user } = setup()
    await user.click(screen.getByText('Open add'))
    await user.type(screen.getByLabelText('Download URL'), 'https://example.com/file.zip')
    await user.type(screen.getByLabelText(/Filename/), ' custom.zip ')
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Free some disk space')
    expect(screen.getByLabelText('Download URL')).toHaveValue('https://example.com/file.zip')
    await user.click(screen.getByRole('button', { name: 'Add download' }))
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2))
    expect(invoke.mock.calls[0][1]).toEqual(invoke.mock.calls[1][1])
    expect(invoke.mock.calls[1][1]).toMatchObject({ input: { suggestedName: 'custom.zip' } })
  })
  it('prevents duplicate submissions while pending', async () => {
    let resolve!: (value: ReturnType<typeof task>) => void
    invoke.mockImplementation(async () => new Promise((done) => {
      resolve = done
    }))
    const { user } = setup()
    await user.click(screen.getByText('Open add'))
    await user.type(screen.getByLabelText('Download URL'), 'https://example.com/file.zip')
    await user.dblClick(screen.getByRole('button', { name: 'Add download' }))
    expect(invoke).toHaveBeenCalledTimes(1)
    expect(screen.getByRole('button', { name: 'Adding…' })).toBeDisabled()
    await user.keyboard('{Escape}')
    expect(screen.getByRole('dialog')).toBeInTheDocument()
    resolve(task())
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  })
  it('closes with Escape and restores trigger focus', async () => {
    const { user } = setup()
    await user.click(screen.getByText('Open add'))
    await user.keyboard('{Escape}')
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(screen.getByText('Open add')).toHaveFocus()
  })
})
