import { os } from '@orpc/server'
import { z } from 'zod'
import { nativeCommands } from '../tauri/commands'
import { downloadTaskSchema } from './schemas'

export const appRouter = {
  system: {
    health: os
      .output(z.literal('ok'))
      .handler(nativeCommands.health),
  },

  downloads: {
    list: os
      .output(z.array(downloadTaskSchema))
      .handler(nativeCommands.listDownloads),
  },
}
