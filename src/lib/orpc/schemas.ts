import { z } from 'zod'

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

export const downloadTaskSchema = z.object({
  id: z.string(),
  url: z.string().url(),
  fileName: z.string(),
  status: downloadStatusSchema,
  downloadedBytes: z.number().int().nonnegative(),
  totalBytes: z.number().int().nonnegative().nullable(),
})

export type DownloadTask = z.infer<typeof downloadTaskSchema>
