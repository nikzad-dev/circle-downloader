import { QueryClient } from '@tanstack/react-query'
import { describe, expect, it, vi } from 'vitest'
import { progress, task } from '@/test/fixtures'
import {
  downloadsKey,
  downloadsListOptions,
  mergeSnapshot,
  mergeTask,
  patchProgress,
  removeTask,
  syncState,
} from './downloads'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))

describe('shared downloads cache', () => {
  it('ignores stale events and preserves other fields', () => {
    const row = task()
    expect(patchProgress(row, progress({ revision: 1 }))).toBe(row)
    expect(patchProgress(row, progress())).toMatchObject({
      downloadedBytes: 200,
      speedBytesPerSecond: 30,
      fileName: row.fileName,
      revision: 2,
    })
  })
  it('cannot rewind progress with a stale mutation result', () => {
    const client = new QueryClient()
    mergeTask(client, task({ revision: 5 }))
    mergeTask(client, task({ revision: 2 }))
    expect(client.getQueryData(downloadsKey)).toEqual([task({ revision: 5 })])
  })
  it('merges a snapshot without losing in-flight creates or newer progress', () => {
    const client = new QueryClient()
    const base = task()
    const fresh = task({ revision: 4, downloadedBytes: 400 })
    const added = task({ id: 'new' })
    expect(
      mergeSnapshot([fresh, added], [base], [base], syncState(client)),
    ).toEqual([fresh, added])
  })
  it('uses equal revision snapshots to enrich failure details', () => {
    const client = new QueryClient()
    const row = task({ status: 'failed', revision: 3 })
    const full = task({
      ...row,
      error: { code: 'DISK_FULL', message: 'full', retryable: false },
    })
    expect(mergeSnapshot([row], [full], [row], syncState(client))).toEqual([
      full,
    ])
  })
  it('does not resurrect removed records from late events or snapshots', () => {
    const client = new QueryClient()
    mergeTask(client, task())
    removeTask(client, 'task-1')
    mergeTask(client, task({ revision: 9 }))
    expect(mergeSnapshot([], [task()], [task()], syncState(client))).toEqual([])
    expect(client.getQueryData(downloadsKey)).toEqual([])
  })
  it('overlays events received before initial snapshot loads', () => {
    const client = new QueryClient()
    syncState(client).events.set('task-1', progress({ revision: 4 }))
    expect(mergeSnapshot([], [task()], [], syncState(client))[0].revision).toBe(
      4,
    )
  })
  it('protects actual query responses from cache/snapshot races', async () => {
    const client = new QueryClient()
    mergeTask(client, task())
    let resolve!: (rows: ReturnType<typeof task>[]) => void
    invoke.mockImplementationOnce(
      async () =>
        new Promise((done) => {
          resolve = done
        }),
    )
    const fetching = client.fetchQuery({
      ...downloadsListOptions(client),
      staleTime: 0,
    })
    await vi.waitFor(() => expect(invoke).toHaveBeenCalled())
    mergeTask(client, task({ revision: 5 }))
    resolve([task()])
    await fetching
    expect(
      client.getQueryData<ReturnType<typeof task>[]>(downloadsKey)?.[0]
        .revision,
    ).toBe(5)
    invoke.mockReset()
    client.clear()
  })
})
