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
    health: os.output(z.literal('ok')).handler(() => nativeCommands.health()),
  },
  downloads: {
    list: os
      .output(z.array(downloadTaskSchema))
      .handler(() => nativeCommands.listDownloads()),
    get: os
      .input(downloadIdInputSchema)
      .output(downloadTaskSchema)
      .handler(({ input }) => nativeCommands.getDownload(input)),
    create: os
      .input(createDownloadInputSchema)
      .output(downloadTaskSchema)
      .handler(({ input }) => nativeCommands.createDownload(input)),
    pause: os
      .input(downloadIdInputSchema)
      .output(downloadTaskSchema)
      .handler(({ input }) => nativeCommands.pauseDownload(input)),
    resume: os
      .input(downloadIdInputSchema)
      .output(downloadTaskSchema)
      .handler(({ input }) => nativeCommands.resumeDownload(input)),
    cancel: os
      .input(downloadIdInputSchema)
      .output(downloadTaskSchema)
      .handler(({ input }) => nativeCommands.cancelDownload(input)),
    remove: os
      .input(removeDownloadInputSchema)
      .output(removeDownloadResultSchema)
      .handler(({ input }) => nativeCommands.removeDownload(input)),
    retry: os
      .input(downloadIdInputSchema)
      .output(downloadTaskSchema)
      .handler(({ input }) => nativeCommands.retryDownload(input)),
  },
  settings: {
    get: os.output(settingsSchema).handler(() => nativeCommands.getSettings()),
    update: os
      .input(updateSettingsInputSchema)
      .output(settingsSchema)
      .handler(({ input }) => nativeCommands.updateSettings(input)),
  },
  queues: {
    list: os.output(z.array(queueSchema)).handler(() => nativeCommands.listQueues()),
  },
}
