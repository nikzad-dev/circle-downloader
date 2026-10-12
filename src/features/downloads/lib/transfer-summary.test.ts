import { expect, it } from 'vitest'
import { task } from '@/test/fixtures'
import { transferPercent, transferTimeLeft } from './transfer-summary'

it('uses bytes for progress and keeps unknown totals indeterminate', () => {
  expect(transferPercent(task())).toBe(10)
  expect(transferPercent(task({ totalBytes: null }))).toBeNull()
  expect(transferPercent(task({ totalBytes: 0 }))).toBeNull()
  expect(transferPercent(task({ downloadedBytes: 999 }))).toBe(99)
  expect(transferPercent(task({ downloadedBytes: 1100 }))).toBe(100)
  expect(transferPercent(task({ status: 'completed', totalBytes: null }))).toBe(
    100,
  )
})
it('only estimates remaining time for an active measurable transfer', () => {
  expect(transferTimeLeft(task())).toBe('45s')
  expect(transferTimeLeft(task({ speedBytesPerSecond: 0 }))).toBe('—')
  expect(transferTimeLeft(task({ status: 'paused' }))).toBe('—')
  expect(transferTimeLeft(task({ totalBytes: null }))).toBe('—')
  expect(
    transferTimeLeft(
      task({ totalBytes: 3700, downloadedBytes: 0, speedBytesPerSecond: 1 }),
    ),
  ).toBe('1h 1m')
})
