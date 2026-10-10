import { z } from 'zod'

/** Byte counts stay JS numbers for MVP; values above MAX_SAFE_INTEGER are rejected. */
const safeByteCount = z
  .number()
  .int()
  .nonnegative()
  .max(Number.MAX_SAFE_INTEGER)

export const downloadStatusSchema = z.enum([
  'queued',
  'connecting',
  'downloading',
  'paused',
  'verifying',
  'completed',
  'failed',
  'canceled',
])

export const errorCodeSchema = z.enum([
  'INVALID_URL',
  'UNSUPPORTED_SCHEME',
  'PATH_NOT_ALLOWED',
  'ALREADY_EXISTS',
  'DISK_FULL',
  'NETWORK_TIMEOUT',
  'HTTP_UNAUTHORIZED',
  'HTTP_FORBIDDEN',
  'HTTP_RATE_LIMIT',
  'RESOURCE_CHANGED',
  'RANGE_UNSUPPORTED',
  'CHECKSUM_MISMATCH',
  'TASK_NOT_FOUND',
  'INVALID_TRANSITION',
  'INTERNAL',
])

export const taskErrorSchema = z.object({
  code: errorCodeSchema,
  message: z.string(),
  retryable: z.boolean(),
})

export const downloadTaskSchema = z.object({
  id: z.string().min(1),
  url: z.string().url(),
  fileName: z.string().min(1),
  destinationId: z.string().min(1),
  status: downloadStatusSchema,
  downloadedBytes: safeByteCount,
  totalBytes: safeByteCount.nullable(),
  speedBytesPerSecond: safeByteCount,
  revision: z.number().int().nonnegative(),
  error: taskErrorSchema.nullable(),
  createdAt: z.string().min(1),
  updatedAt: z.string().min(1),
})

export const createDownloadInputSchema = z.object({
  url: z.string().url(),
  destinationId: z.string().min(1).optional(),
  suggestedName: z.string().min(1).optional(),
  startNow: z.boolean().optional(),
  requestId: z.string().min(1).optional(),
})

export const downloadIdInputSchema = z.object({
  id: z.string().min(1),
})

export const removeDownloadInputSchema = z.object({
  id: z.string().min(1),
  deletePartial: z.boolean().optional(),
})

export const removeDownloadResultSchema = z.object({
  removed: z.literal(true),
})

export const progressEventSchema = z.object({
  taskId: z.string().min(1),
  revision: z.number().int().nonnegative(),
  transferredBytes: safeByteCount,
  totalBytes: safeByteCount.nullable(),
  bytesPerSecond: safeByteCount,
  status: downloadStatusSchema,
  timestamp: z.string().min(1),
})

export const settingsSchema = z.object({
  maxConcurrentDownloads: z.number().int().min(1).max(32),
  defaultDestinationId: z.string().min(1),
  autoStart: z.boolean(),
})

export const updateSettingsInputSchema = settingsSchema.partial()

export const queueSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  taskCount: z.number().int().nonnegative(),
})

export const appErrorSchema = z.object({
  code: errorCodeSchema,
  message: z.string(),
  retryable: z.boolean(),
})

export type DownloadStatus = z.infer<typeof downloadStatusSchema>
export type ErrorCode = z.infer<typeof errorCodeSchema>
export type DownloadTask = z.infer<typeof downloadTaskSchema>
export type CreateDownloadInput = z.infer<typeof createDownloadInputSchema>
export type ProgressEvent = z.infer<typeof progressEventSchema>
export type Settings = z.infer<typeof settingsSchema>
export type Queue = z.infer<typeof queueSchema>
export type AppError = z.infer<typeof appErrorSchema>
