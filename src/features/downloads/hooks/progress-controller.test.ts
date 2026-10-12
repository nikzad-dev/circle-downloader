import { QueryClient } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { progress, task } from '@/test/fixtures'
import { downloadsKey, mergeTask, removeTask } from '../api/downloads'
import { createProgressController } from './progress-controller'

describe('progress controller', () => {
  let client: QueryClient
  beforeEach(() => {
    vi.useFakeTimers()
    client = new QueryClient()
    mergeTask(client, task())
  })
  afterEach(() => {
    client.clear()
    vi.useRealTimers()
  })
  it('flushes the final event at 10 Hz and ignores older revisions', () => {
    const controller = createProgressController(client)
    const writes = vi.spyOn(client, 'setQueryData')
    for (let revision = 2; revision <= 100; revision++)
      controller.receive(progress({ revision, transferredBytes: revision }))
    controller.receive(progress({ revision: 4, transferredBytes: 0 }))
    expect(writes).not.toHaveBeenCalled()
    vi.advanceTimersByTime(100)
    expect(writes).toHaveBeenCalledTimes(1)
    expect(
      client.getQueryData<ReturnType<typeof task>[]>(downloadsKey)?.[0]
        .downloadedBytes,
    ).toBe(100)
    controller.dispose()
  })
  it('applies status changes immediately and reconciles failure DTOs', () => {
    const fetch = vi.spyOn(client, 'fetchQuery').mockResolvedValue([])
    const controller = createProgressController(client)
    controller.receive(progress({ status: 'failed' }))
    expect(
      client.getQueryData<ReturnType<typeof task>[]>(downloadsKey)?.[0].status,
    ).toBe('failed')
    vi.advanceTimersByTime(250)
    expect(fetch).toHaveBeenCalledTimes(1)
    controller.dispose()
  })
  it('coalesces genuine gaps without treating batched sequential events as gaps', () => {
    const fetch = vi.spyOn(client, 'fetchQuery').mockResolvedValue([])
    const controller = createProgressController(client)
    controller.receive(progress())
    controller.receive(progress({ revision: 3 }))
    vi.advanceTimersByTime(250)
    expect(fetch).not.toHaveBeenCalled()
    controller.receive(progress({ revision: 8 }))
    controller.receive(progress({ taskId: 'unknown', revision: 1 }))
    vi.advanceTimersByTime(250)
    expect(fetch).toHaveBeenCalledTimes(1)
    controller.dispose()
  })
  it('ignores removed tasks and cancels all pending work on cleanup', () => {
    const fetch = vi.spyOn(client, 'fetchQuery').mockResolvedValue([])
    const controller = createProgressController(client)
    controller.receive(progress({ revision: 9 }))
    removeTask(client, 'task-1')
    controller.receive(progress({ revision: 10 }))
    controller.dispose()
    vi.runAllTimers()
    expect(client.getQueryData(downloadsKey)).toEqual([])
    expect(fetch).not.toHaveBeenCalled()
  })
  it('bounds cache writes across 50 concurrent tasks and retains unchanged rows', () => {
    const rows = Array.from({ length: 51 }, (_, index) =>
      task({ id: `task-${index}`, revision: 1 }),
    )
    client.setQueryData(downloadsKey, rows)
    const unchanged =
      client.getQueryData<ReturnType<typeof task>[]>(downloadsKey)![50]
    const controller = createProgressController(client)
    const writes = vi.spyOn(client, 'setQueryData')
    for (let revision = 2; revision <= 101; revision++) {
      for (let index = 0; index < 50; index++)
        controller.receive(
          progress({
            taskId: `task-${index}`,
            revision,
            transferredBytes: revision,
          }),
        )
      if (revision % 10 === 1) vi.advanceTimersByTime(100)
    }
    expect(writes).toHaveBeenCalledTimes(10)
    const updated =
      client.getQueryData<ReturnType<typeof task>[]>(downloadsKey)!
    expect(
      updated
        .slice(0, 50)
        .every((row) => row.revision === 101 && row.downloadedBytes === 101),
    ).toBe(true)
    expect(updated[50]).toBe(unchanged)
    controller.dispose()
  })
})
