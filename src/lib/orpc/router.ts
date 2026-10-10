import { os } from '@orpc/server'
import { z } from 'zod'
import { nativeCommands } from '../tauri/commands'
import {
  createDownloadInputSchema,
  downloadIdInputSchema,
  downloadTaskSchema,
  queueSchema,
  removeDownloadInputSchema,
  removeDownloadResultSchema,
  settingsSchema,
  updateSettingsInputSchema,
} from './schemas'

export const appRouter = {
  system: {
    health: os.output(z.literal('ok')).handler(async () => nativeCommands.health()),
  },
  downloads: {
    list: os
      .output(z.array(downloadTaskSchema))
      .handler(async () => nativeCommands.listDownloads()),
    get: os
      .input(downloadIdInputSchema)
      .output(downloadTaskSchema)
      .handler(async ({ input }) => nativeCommands.getDownload(input)),
    create: os
      .input(createDownloadInputSchema)
      .output(downloadTaskSchema)
      .handler(async ({ input }) => nativeCommands.createDownload(input)),
    pause: os
      .input(downloadIdInputSchema)
      .output(downloadTaskSchema)
      .handler(async ({ input }) => nativeCommands.pauseDownload(input)),
    resume: os
      .input(downloadIdInputSchema)
      .output(downloadTaskSchema)
      .handler(async ({ input }) => nativeCommands.resumeDownload(input)),
    cancel: os
      .input(downloadIdInputSchema)
      .output(downloadTaskSchema)
      .handler(async ({ input }) => nativeCommands.cancelDownload(input)),
    remove: os
      .input(removeDownloadInputSchema)
      .output(removeDownloadResultSchema)
      .handler(async ({ input }) => nativeCommands.removeDownload(input)),
    retry: os
      .input(downloadIdInputSchema)
      .output(downloadTaskSchema)
      .handler(async ({ input }) => nativeCommands.retryDownload(input)),
  },
  settings: {
    get: os.output(settingsSchema).handler(async () => nativeCommands.getSettings()),
    update: os
      .input(updateSettingsInputSchema)
      .output(settingsSchema)
      .handler(async ({ input }) => nativeCommands.updateSettings(input)),
  },
  queues: {
    list: os.output(z.array(queueSchema)).handler(async () => nativeCommands.listQueues()),
  },
}
