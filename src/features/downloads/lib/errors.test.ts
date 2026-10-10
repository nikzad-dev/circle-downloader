import { describe, expect, it } from 'vitest'
import { NativeCommandError } from '@/lib/tauri/errors'
import { downloadErrorMessage } from './errors'

describe('actionable transfer failures', () => {
  it.each([
    ['TLS_ERROR', 'secure connection'],
    ['NETWORK_CONNECTION', 'download host'],
    ['NETWORK_INTERRUPTED', 'before the download finished'],
    ['HTTP_NOT_FOUND', 'new download link'],
    ['HTTP_CLIENT_ERROR', 'server rejected'],
    ['HTTP_SERVER_ERROR', 'retry'],
    ['HTTP_REDIRECT', 'direct file link'],
    ['STORAGE_PERMISSION', 'read-only'],
    ['STORAGE_NOT_FOUND', 'partial file is missing'],
    ['STORAGE_ERROR', 'drive is connected'],
    ['DATA_DIRECTORY_IN_USE', 'Another Circle process'],
  ] as const)('explains %s without displaying raw diagnostic secrets', (code, action) => {
    const error = new NativeCommandError({ code, message: 'https://private.example/file?token=secret', retryable: false })
    const message = downloadErrorMessage(error)
    expect(message).toContain(action)
    expect(message).not.toContain('secret')
    expect(message).not.toContain('private.example')
  })
})
