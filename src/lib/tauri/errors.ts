import type { AppError, ErrorCode } from '../orpc/schemas'
import { appErrorSchema } from '../orpc/schemas'

export class NativeCommandError extends Error {
  readonly code: ErrorCode
  readonly retryable: boolean

  constructor(error: AppError) {
    super(error.message)
    this.name = 'NativeCommandError'
    this.code = error.code
    this.retryable = error.retryable
  }
}

export function normalizeNativeError(error: unknown): never {
  if (error instanceof NativeCommandError)
    throw error

  if (typeof error === 'string') {
    try {
      const parsed = appErrorSchema.parse(JSON.parse(error))
      throw new NativeCommandError(parsed)
    }
    catch (inner) {
      if (inner instanceof NativeCommandError)
        throw inner
    }
  }

  if (typeof error === 'object' && error !== null) {
    const record = error as Record<string, unknown>
    // Tauri often wraps as { message: string } containing JSON
    if (typeof record.message === 'string') {
      try {
        const parsed = appErrorSchema.parse(JSON.parse(record.message))
        throw new NativeCommandError(parsed)
      }
      catch (inner) {
        if (inner instanceof NativeCommandError)
          throw inner
      }
    }

    const direct = appErrorSchema.safeParse(error)
    if (direct.success)
      throw new NativeCommandError(direct.data)
  }

  throw new NativeCommandError({
    code: 'INTERNAL',
    message: error instanceof Error ? error.message : 'Unknown native error',
    retryable: false,
  })
}
