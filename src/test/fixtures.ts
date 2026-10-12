import type { DownloadTask, ProgressEvent } from '@/lib/orpc/schemas'

export function task(overrides: Partial<DownloadTask> = {}): DownloadTask {
  return {
    expectedSha256: null,
    actualSha256: null,
    checksumResult: null,
    retryCount: 0,
    nextRetryAt: null,
    transferMode: 'single',
    id: 'task-1',
    url: 'https://example.com/file.zip',
    fileName: 'file.zip',
    destinationId: 'default',
    queueId: 'default',
    status: 'downloading',
    downloadedBytes: 100,
    totalBytes: 1000,
    speedBytesPerSecond: 20,
    revision: 1,
    error: null,
    createdAt: '2026-10-10T10:00:00Z',
    updatedAt: '2026-10-10T10:00:00Z',
    ...overrides,
  }
}
export function progress(
  overrides: Partial<ProgressEvent> = {},
): ProgressEvent {
  return {
    taskId: 'task-1',
    revision: 2,
    transferredBytes: 200,
    totalBytes: 1000,
    bytesPerSecond: 30,
    status: 'downloading',
    timestamp: '2026-10-10T10:00:01Z',
    ...overrides,
  }
}
