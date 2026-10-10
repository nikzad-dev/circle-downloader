import type { DownloadStatus } from '@/lib/orpc/schemas'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { task } from '@/test/fixtures'
import { downloadsKey, mergeTask } from '../api/downloads'
import { TaskActions } from './task-actions'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
function setup(status: DownloadStatus = 'downloading') {
  const client = new QueryClient()
  const row = task({ status })
  mergeTask(client, row)
  render(<QueryClientProvider client={client}><TaskActions task={row} /></QueryClientProvider>)
  return { client, user: userEvent.setup() }
}
beforeEach(() => {
  invoke.mockReset()
})
describe('task actions', () => {
  it.each([['downloading', 'Pause', 'pause_download'], ['paused', 'Resume', 'resume_download'], ['failed', 'Retry', 'retry_download'], ['verifying', 'Cancel', 'cancel_download']] as const)('calls the correct action in %s', async (status, label, command) => {
    invoke.mockResolvedValue(task({ revision: 2 }))
    const { user } = setup(status)
    await user.click(screen.getByRole('button', { name: `${label} file.zip` }))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith(command, { id: 'task-1' }))
  })
  it.each(['completed', 'canceled'] as const)('only offers removal for %s', (status) => {
    setup(status)
    expect(screen.getAllByRole('button')).toHaveLength(1)
    expect(screen.getByRole('button', { name: 'Remove file.zip' })).toBeEnabled()
  })
  it('requires confirmation and honors partial-file retention', async () => {
    invoke.mockResolvedValue({ removed: true })
    const { user, client } = setup('paused')
    await user.click(screen.getByRole('button', { name: 'Remove file.zip' }))
    expect(invoke).not.toHaveBeenCalled()
    expect(screen.getByRole('alertdialog')).toHaveTextContent('Completed files stay on disk')
    await waitFor(() => expect(screen.getByRole('button', { name: 'Keep download' })).toHaveFocus())
    await user.click(screen.getByRole('checkbox', { name: 'Delete the partial file' }))
    await user.click(screen.getByRole('button', { name: 'Remove download' }))
    await waitFor(() => expect(client.getQueryData(downloadsKey)).toEqual([]))
    expect(invoke).toHaveBeenCalledWith('remove_download', { input: { id: 'task-1', deletePartial: false } })
  })
  it('keeps the record when removal fails and shows recovery copy', async () => {
    invoke.mockRejectedValue({ code: 'INVALID_TRANSITION', message: 'changed', retryable: false })
    const { user, client } = setup()
    await user.click(screen.getByRole('button', { name: 'Remove file.zip' }))
    await user.click(screen.getByRole('button', { name: 'Remove download' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('download changed')
    expect(client.getQueryData(downloadsKey)).toHaveLength(1)
    expect(screen.getByRole('alertdialog')).toBeInTheDocument()
  })
  it('disables conflicting row actions while pending', async () => {
    let resolve!: (value: ReturnType<typeof task>) => void
    invoke.mockImplementation(async () => new Promise((done) => {
      resolve = done
    }))
    const { user } = setup()
    await user.click(screen.getByRole('button', { name: 'Pause file.zip' }))
    expect(screen.getByRole('button', { name: 'Cancel file.zip' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Remove file.zip' })).toBeDisabled()
    resolve(task({ status: 'paused', revision: 2 }))
    await waitFor(() => expect(screen.getByRole('button', { name: 'Remove file.zip' })).toBeEnabled())
  })
})
