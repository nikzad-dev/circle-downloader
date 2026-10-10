import { invoke } from '@tauri-apps/api/core'
import { z } from 'zod'
import { downloadTaskSchema } from '../orpc/schemas'

// Keep all native IPC calls in one small, testable boundary.
// Output parsing detects accidental TS/Rust contract mismatches.
export const nativeCommands = {
  health: async (): Promise<'ok'> => {
    return z.literal('ok').parse(await invoke<unknown>('health_check'))
  },

  listDownloads: async () => {
    const result = await invoke<unknown>('list_downloads')
    return z.array(downloadTaskSchema).parse(result)
  },
}
