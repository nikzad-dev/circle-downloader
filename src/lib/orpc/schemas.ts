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
  'LENGTH_MISMATCH',
  'URL_NOT_ALLOWED',
  'TASK_NOT_FOUND',
  'INVALID_TRANSITION',
  'DATA_DIRECTORY_IN_USE',
  'NETWORK_CONNECTION',
  'NETWORK_INTERRUPTED',
  'TLS_ERROR',
  'HTTP_NOT_FOUND',
  'HTTP_CLIENT_ERROR',
  'HTTP_SERVER_ERROR',
  'HTTP_REDIRECT',
  'STORAGE_PERMISSION',
  'STORAGE_NOT_FOUND',
  'STORAGE_ERROR',
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
  expectedSha256: z
    .string()
    .regex(/^[a-f0-9]{64}$/)
    .nullable()
    .default(null),
  actualSha256: z
    .string()
    .regex(/^[a-f0-9]{64}$/)
    .nullable()
    .default(null),
  checksumResult: z
    .enum(['computed', 'matched', 'mismatch'])
    .nullable()
    .default(null),
  retryCount: z.number().int().nonnegative(),
  nextRetryAt: z.string().min(1).nullable(),
  transferMode: z.enum(['single', 'segmented']).default('single'),
  createdAt: z.string().min(1),
  updatedAt: z.string().min(1),
})

export const createDownloadInputSchema = z.object({
  url: z.string().url(),
  destinationId: z.string().min(1).optional(),
  suggestedName: z.string().min(1).optional(),
  startNow: z.boolean().optional(),
  requestId: z.string().min(1).optional(),
  expectedSha256: z
    .string()
    .trim()
    .regex(/^[a-f0-9]{64}$/i)
    .optional(),
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
  maxConnectionsPerDownload: z.number().int().min(1).max(4),
})

export const updateSettingsInputSchema = settingsSchema.partial()

export const queueSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  taskCount: z.number().int().nonnegative(),
})

export const destinationSchema = z.object({
  id: z.string().min(1),
  label: z.string().min(1),
  displayPath: z.string().min(1),
  isDefault: z.boolean(),
})

export const grantDestinationInputSchema = z.object({
  path: z.string().min(1),
  label: z.string().min(1).optional(),
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
export type Destination = z.infer<typeof destinationSchema>
export type AppError = z.infer<typeof appErrorSchema>
