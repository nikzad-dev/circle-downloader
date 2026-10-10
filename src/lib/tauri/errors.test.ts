import { describe, expect, it } from 'vitest'
import { NativeCommandError, normalizeNativeError } from './errors'

describe('normalizeNativeError', () => {
  it('parses JSON string app errors', () => {
    expect(() =>
      normalizeNativeError(
        JSON.stringify({
          code: 'TASK_NOT_FOUND',
          message: 'Download not found',
          retryable: false,
        }),
      ),
    ).toThrow(NativeCommandError)

    try {
      normalizeNativeError(
        JSON.stringify({
          code: 'INVALID_URL',
          message: 'bad',
          retryable: false,
        }),
      )
    }
    catch (error) {
      expect(error).toBeInstanceOf(NativeCommandError)
      expect((error as NativeCommandError).code).toBe('INVALID_URL')
    }
  })

  it('parses Tauri-wrapped message JSON', () => {
    try {
      normalizeNativeError({
        message: JSON.stringify({
          code: 'UNSUPPORTED_SCHEME',
          message: 'http not allowed',
          retryable: false,
        }),
      })
    }
    catch (error) {
      expect(error).toBeInstanceOf(NativeCommandError)
      expect((error as NativeCommandError).code).toBe('UNSUPPORTED_SCHEME')
    }
  })

  it('falls back to INTERNAL', () => {
    try {
      normalizeNativeError(new Error('boom'))
    }
    catch (error) {
      expect(error).toBeInstanceOf(NativeCommandError)
      expect((error as NativeCommandError).code).toBe('INTERNAL')
    }
  })
})
