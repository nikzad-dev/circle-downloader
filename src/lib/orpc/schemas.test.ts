import { describe, expect, it } from 'vitest'
import {
  createDownloadInputSchema,
  downloadTaskSchema,
  errorCodeSchema,
  progressEventSchema,
  settingsSchema,
} from './schemas'

const sampleTask = {
  id: '01HXYZ',
  url: 'https://example.com/file.bin',
  expectedSha256: null,
  actualSha256: null,
  checksumResult: null,
  fileName: 'file.bin',
  destinationId: 'default',
  status: 'queued' as const,
  downloadedBytes: 0,
  totalBytes: 1024,
  speedBytesPerSecond: 0,
  revision: 0,
  error: null,
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
}

describe('downloadTaskSchema', () => {
  it('accepts a full blueprint DTO', () => {
    expect(downloadTaskSchema.parse(sampleTask)).toMatchObject({
      id: '01HXYZ',
      status: 'queued',
      destinationId: 'default',
    })
  })

  it('rejects byte counts above MAX_SAFE_INTEGER', () => {
    expect(() =>
      downloadTaskSchema.parse({
        ...sampleTask,
        downloadedBytes: Number.MAX_SAFE_INTEGER + 1,
      }),
    ).toThrow()
  })

  it('accepts task errors with stable codes', () => {
    expect(
      downloadTaskSchema.parse({
        ...sampleTask,
        status: 'failed',
        error: {
          code: 'TASK_NOT_FOUND',
          message: 'missing',
          retryable: false,
        },
      }).error?.code,
    ).toBe('TASK_NOT_FOUND')
  })
})

describe('createDownloadInputSchema', () => {
  it('accepts url-only input', () => {
    expect(
      createDownloadInputSchema.parse({ url: 'https://example.com/a' }),
    ).toMatchObject({ url: 'https://example.com/a' })
  })

  it('rejects invalid urls', () => {
    expect(() => createDownloadInputSchema.parse({ url: 'not-a-url' })).toThrow()
  })
})

describe('errorCodeSchema', () => {
  it('includes blueprint error codes', () => {
    for (const code of [
      'INVALID_URL',
      'UNSUPPORTED_SCHEME',
      'TASK_NOT_FOUND',
      'RESOURCE_CHANGED',
      'RANGE_UNSUPPORTED',
    ] as const) {
      expect(errorCodeSchema.parse(code)).toBe(code)
    }
  })
})

describe('progressEventSchema', () => {
  it('accepts revisioned progress payloads', () => {
    expect(
      progressEventSchema.parse({
        taskId: '1',
        revision: 3,
        transferredBytes: 10,
        totalBytes: 100,
        bytesPerSecond: 5,
        status: 'downloading',
        timestamp: '2026-01-01T00:00:01.000Z',
      }),
    ).toMatchObject({ revision: 3 })
  })
})

describe('settingsSchema', () => {
  it('accepts default settings shape', () => {
    expect(
      settingsSchema.parse({
        maxConcurrentDownloads: 3,
        defaultDestinationId: 'default',
        autoStart: true,
      }),
    ).toMatchObject({ maxConcurrentDownloads: 3 })
  })
})
