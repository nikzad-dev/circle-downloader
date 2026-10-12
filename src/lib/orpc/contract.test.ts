import { readFileSync } from 'node:fs'
import { expect, it } from 'vitest'
import { z } from 'zod'
import {
  appErrorSchema,
  downloadTaskSchema,
  errorCodeSchema,
  progressEventSchema,
  queueSchema,
  removeDownloadResultSchema,
  settingsSchema,
} from './schemas'

it('accepts the exact native serialization fixture without dropping fields', () => {
  const fixture: unknown = JSON.parse(
    readFileSync(
      new URL('../../../tests/fixtures/ipc-contract.json', import.meta.url),
      'utf8',
    ),
  )
  const contract = z
    .object({
      task: downloadTaskSchema.strict(),
      progress: progressEventSchema.strict(),
      settings: settingsSchema.strict(),
      queue: queueSchema.strict(),
      remove: removeDownloadResultSchema.strict(),
      error: appErrorSchema.strict(),
      errorCodes: z.array(errorCodeSchema),
    })
    .strict()
  const parsed = contract.parse(fixture)
  expect(parsed).toEqual(fixture)
  expect(parsed.errorCodes).toEqual(errorCodeSchema.options)
})
