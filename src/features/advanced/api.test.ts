import { beforeEach, describe, expect, it, vi } from 'vitest'
import { advanced, emptyTaskOptions } from './api'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeEach(() => invoke.mockReset())

describe('advanced native boundary', () => {
  it('rejects oversized imports before invoking native code', async () => {
    await expect(advanced.import({ urls: Array.from({ length: 201 }, (_, index) => `https://example.com/file-${index}`), options: emptyTaskOptions, startNow: false, batchId: 'batch' })).rejects.toThrow()
    expect(invoke).not.toHaveBeenCalled()
  })
  it('validates queue concurrency and rates before saving', async () => {
    await expect(advanced.saveQueue({ id: 'default', name: 'Main', paused: false, maxConcurrent: 0, bytesPerSecond: 0, taskCount: 0 })).rejects.toThrow()
    expect(invoke).not.toHaveBeenCalled()
  })
  it('returns only scoped grants even if native output includes a secret', async () => {
    invoke.mockResolvedValue([{ id: 'opaque', kind: 'origin', scope: 'https://example.com', password: 'must-never-return', username: 'private' }])
    const grants = await advanced.grants()
    expect(grants).toEqual([{ id: 'opaque', kind: 'origin', scope: 'https://example.com' }])
    expect(JSON.stringify(grants)).not.toContain('private')
  })
  it('forwards persisted measurements without substituting sample data', async () => {
    const recorded = { transferredBytes: 2048, transferMillis: 1000, completedAttempts: 1, failedAttempts: 0, days: [{ day: '2026-10-11', transferredBytes: 2048, transferMillis: 1000 }] }
    invoke.mockResolvedValue(recorded)
    expect(await advanced.statistics()).toEqual(recorded)
    expect(invoke).toHaveBeenCalledWith('get_statistics', undefined)
  })
})
