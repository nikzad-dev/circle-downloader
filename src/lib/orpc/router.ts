import { os } from '@orpc/server'
import { z } from 'zod'
import { nativeCommands } from '../tauri/commands'
import {
  createDownloadInputSchema,
  destinationSchema,
  downloadIdInputSchema,
  downloadTaskSchema,
  grantDestinationInputSchema,
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
    open: os
      .input(downloadIdInputSchema)
      .handler(async ({ input }) => nativeCommands.openDownload(input)),
    reveal: os
      .input(downloadIdInputSchema)
      .handler(async ({ input }) => nativeCommands.revealDownload(input)),
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
  destinations: {
    list: os.output(z.array(destinationSchema)).handler(async () => nativeCommands.listDestinations()),
    grant: os
      .input(grantDestinationInputSchema)
      .output(destinationSchema)
      .handler(async ({ input }) => nativeCommands.grantDestination(input)),
    remove: os
      .input(downloadIdInputSchema)
      .handler(async ({ input }) => nativeCommands.removeDestination(input)),
  },
}
