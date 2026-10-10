import type { CreateDownloadInput, Settings } from '../orpc/schemas'
import { invoke } from '@tauri-apps/api/core'
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
} from '../orpc/schemas'
import { normalizeNativeError } from './errors'

async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(command, args)
  }
  catch (error) {
    normalizeNativeError(error)
  }
}

// Keep all native IPC calls in one small, testable boundary.
// Output parsing detects accidental TS/Rust contract mismatches.
export const nativeCommands = {
  health: async (): Promise<'ok'> => {
    const result = await invokeCommand<unknown>('health_check')
    if (result !== 'ok') {
      throw new Error(`Unexpected health_check result: ${String(result)}`)
    }
    return 'ok'
  },

  listDownloads: async () => {
    const result = await invokeCommand<unknown>('list_downloads')
    return downloadTaskSchema.array().parse(result)
  },

  getDownload: async (input: { id: string }) => {
    const parsed = downloadIdInputSchema.parse(input)
    const result = await invokeCommand<unknown>('get_download', parsed)
    return downloadTaskSchema.parse(result)
  },

  createDownload: async (input: CreateDownloadInput) => {
    const parsed = createDownloadInputSchema.parse(input)
    const result = await invokeCommand<unknown>('create_download', {
      input: {
        ...parsed,
        startNow: parsed.startNow ?? true,
      },
    })
    return downloadTaskSchema.parse(result)
  },

  pauseDownload: async (input: { id: string }) => {
    const parsed = downloadIdInputSchema.parse(input)
    const result = await invokeCommand<unknown>('pause_download', parsed)
    return downloadTaskSchema.parse(result)
  },

  resumeDownload: async (input: { id: string }) => {
    const parsed = downloadIdInputSchema.parse(input)
    const result = await invokeCommand<unknown>('resume_download', parsed)
    return downloadTaskSchema.parse(result)
  },

  cancelDownload: async (input: { id: string }) => {
    const parsed = downloadIdInputSchema.parse(input)
    const result = await invokeCommand<unknown>('cancel_download', parsed)
    return downloadTaskSchema.parse(result)
  },

  removeDownload: async (input: { id: string, deletePartial?: boolean }) => {
    const parsed = removeDownloadInputSchema.parse(input)
    const result = await invokeCommand<unknown>('remove_download', {
      input: {
        ...parsed,
        deletePartial: parsed.deletePartial ?? true,
      },
    })
    return removeDownloadResultSchema.parse(result)
  },

  retryDownload: async (input: { id: string }) => {
    const parsed = downloadIdInputSchema.parse(input)
    const result = await invokeCommand<unknown>('retry_download', parsed)
    return downloadTaskSchema.parse(result)
  },

  getSettings: async (): Promise<Settings> => {
    const result = await invokeCommand<unknown>('get_settings')
    return settingsSchema.parse(result)
  },

  updateSettings: async (input: Partial<Settings>) => {
    const parsed = updateSettingsInputSchema.parse(input)
    const result = await invokeCommand<unknown>('update_settings', { input: parsed })
    return settingsSchema.parse(result)
  },

  listQueues: async () => {
    const result = await invokeCommand<unknown>('list_queues')
    return queueSchema.array().parse(result)
  },

  listDestinations: async () => {
    const result = await invokeCommand<unknown>('list_destinations')
    return destinationSchema.array().parse(result)
  },

  grantDestination: async (input: { path: string, label?: string }) => {
    const parsed = grantDestinationInputSchema.parse(input)
    const result = await invokeCommand<unknown>('grant_destination', { input: parsed })
    return destinationSchema.parse(result)
  },

  removeDestination: async (input: { id: string }) => {
    const parsed = downloadIdInputSchema.parse(input)
    await invokeCommand<unknown>('remove_destination', parsed)
  },

  openDownload: async (input: { id: string }) => {
    const parsed = downloadIdInputSchema.parse(input)
    await invokeCommand<unknown>('open_download', parsed)
  },

  revealDownload: async (input: { id: string }) => {
    const parsed = downloadIdInputSchema.parse(input)
    await invokeCommand<unknown>('reveal_download', parsed)
  },
}
