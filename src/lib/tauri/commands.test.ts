import { beforeEach, describe, expect, it, vi } from 'vitest'

const invoke = vi.fn<(...args: unknown[]) => Promise<unknown>>()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (...args: unknown[]): Promise<unknown> => invoke(...args),
}))

const { nativeCommands } = await import('./commands')

const sampleTask = {
  id: 'task-1',
  url: 'https://example.com/file.bin',
  expectedSha256: null,
  actualSha256: null,
  checksumResult: null,
  fileName: 'file.bin',
  destinationId: 'default',
  status: 'queued',
  downloadedBytes: 0,
  totalBytes: null,
  speedBytesPerSecond: 0,
  revision: 1,
  error: null,
  retryCount: 0,
  nextRetryAt: null,
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
}

describe('nativeCommands', () => {
  beforeEach(() => {
    invoke.mockReset()
  })

  it('health returns ok', async () => {
    invoke.mockResolvedValueOnce('ok')
    await expect(nativeCommands.health()).resolves.toBe('ok')
    expect(invoke).toHaveBeenCalledWith('health_check', undefined)
  })

  it('listDownloads parses task array', async () => {
    invoke.mockResolvedValueOnce([sampleTask])
    await expect(nativeCommands.listDownloads()).resolves.toHaveLength(1)
    expect(invoke).toHaveBeenCalledWith('list_downloads', undefined)
  })

  it('createDownload forwards validated input', async () => {
    invoke.mockResolvedValueOnce(sampleTask)
    await nativeCommands.createDownload({
      url: 'https://example.com/file.bin',
      suggestedName: 'file.bin',
    })
    expect(invoke).toHaveBeenCalledTimes(1)
    const [, args] = invoke.mock.calls[0] ?? []
    expect(args).toEqual({
      input: {
        url: 'https://example.com/file.bin',
        suggestedName: 'file.bin',
        startNow: true,
      },
    })
  })

  it('getDownload normalizes TASK_NOT_FOUND', async () => {
    invoke.mockRejectedValueOnce({
      message: JSON.stringify({
        code: 'TASK_NOT_FOUND',
        message: 'missing',
        retryable: false,
      }),
    })
    await expect(nativeCommands.getDownload({ id: 'missing' })).rejects.toMatchObject({
      code: 'TASK_NOT_FOUND',
    })
  })

  it('getSettings parses settings', async () => {
    invoke.mockResolvedValueOnce({
      maxConcurrentDownloads: 2,
      defaultDestinationId: 'default',
      autoStart: true,
      maxConnectionsPerDownload: 1,
    })
    await expect(nativeCommands.getSettings()).resolves.toMatchObject({
      maxConcurrentDownloads: 2,
      maxConnectionsPerDownload: 1,
    })
  })

  it('listQueues parses queues', async () => {
    invoke.mockResolvedValueOnce([{ id: 'default', name: 'Default', taskCount: 0 }])
    await expect(nativeCommands.listQueues()).resolves.toEqual([
      { id: 'default', name: 'Default', taskCount: 0 },
    ])
  })
})
