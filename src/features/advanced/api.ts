import { useQuery } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { z } from 'zod'
import { createDownloadInputSchema, downloadTaskSchema } from '@/lib/orpc/schemas'
import { normalizeNativeError } from '@/lib/tauri/errors'

const rate = z.number().int().min(0).max(1_000_000_000)
export const scheduleSchema = z.object({ localTime: z.string(), timeZone: z.string(), ambiguity: z.enum(['earlier', 'later']) })
export const taskOptionsSchema = z.object({ id: z.string(), queueId: z.string(), priority: z.number().int().min(-2).max(2), category: z.string().max(80), resolvedCategory: z.string().optional(), tags: z.array(z.string().min(1).max(40)).max(20), schedule: scheduleSchema.nullable(), scheduledAt: z.string().nullable(), credentialId: z.string().nullable(), connections: z.number().int().min(1).max(4).nullable().optional() })
export const queuePolicySchema = z.object({ id: z.string(), name: z.string().min(1).max(80), paused: z.boolean(), maxConcurrent: z.number().int().min(1).max(32), bytesPerSecond: rate, taskCount: z.number().int().nonnegative() })
export const networkSettingsSchema = z.object({ bytesPerSecond: rate, proxyUrl: z.string(), proxyCredentialId: z.string().nullable(), connectTimeoutSecs: z.number().int().min(1).max(120) })
export const credentialGrantSchema = z.object({ id: z.string(), kind: z.enum(['origin', 'proxy']), scope: z.string() })
export const statisticsSchema = z.object({ transferredBytes: z.number().nonnegative(), transferMillis: z.number().nonnegative(), completedAttempts: z.number().int().nonnegative(), failedAttempts: z.number().int().nonnegative(), days: z.array(z.object({ day: z.string(), transferredBytes: z.number().nonnegative(), transferMillis: z.number().nonnegative() })) })
export type TaskOptions = z.infer<typeof taskOptionsSchema>
export type QueuePolicy = z.infer<typeof queuePolicySchema>
export type NetworkSettings = z.infer<typeof networkSettingsSchema>
export const emptyTaskOptions: TaskOptions = { id: '', queueId: 'default', priority: 0, category: '', tags: [], schedule: null, scheduledAt: null, credentialId: null }

async function call<T>(command: string, schema: z.ZodType<T>, args?: Record<string, unknown>): Promise<T> {
  try {
    return schema.parse(await invoke(command, args))
  }
  catch (error) { normalizeNativeError(error) }
}
export const advanced = {
  create: async (input: z.infer<typeof createDownloadInputSchema>, options: TaskOptions) => call('create_configured_download', downloadTaskSchema, { input: createDownloadInputSchema.parse(input), options: taskOptionsSchema.parse(options) }),
  tasks: async () => call('get_task_options', taskOptionsSchema.array()),
  updateTask: async (input: TaskOptions) => call('update_task_options', taskOptionsSchema, { input: taskOptionsSchema.parse(input) }),
  queues: async () => call('get_queue_policies', queuePolicySchema.array()),
  saveQueue: async (input: QueuePolicy) => call('save_queue_policy', queuePolicySchema, { input: queuePolicySchema.parse(input) }),
  network: async () => call('get_network_settings', networkSettingsSchema),
  saveNetwork: async (input: NetworkSettings) => call('save_network_settings', networkSettingsSchema, { input: networkSettingsSchema.parse(input) }),
  grants: async () => call('list_credential_grants', credentialGrantSchema.array()),
  saveCredential: async (input: { kind: 'origin' | 'proxy', scope: string, username: string, password: string }) => call('save_credential', credentialGrantSchema, { input: z.object({ kind: z.enum(['origin', 'proxy']), scope: z.string().url(), username: z.string().max(256), password: z.string().min(1).max(4096) }).parse(input) }),
  revokeCredential: async (id: string) => call('revoke_credential', z.null(), { id }),
  statistics: async () => call('get_statistics', statisticsSchema),
  import: async (input: { urls: string[], options: TaskOptions, startNow: boolean, batchId: string }) => call('bulk_import', z.object({ created: downloadTaskSchema.array(), errors: z.string().array() }), { input: z.object({ urls: z.array(z.string().trim().url()).min(1).max(200), options: taskOptionsSchema, startNow: z.boolean(), batchId: z.string().min(1).max(80) }).parse(input) }),
}
export const advancedKeys = { tasks: ['advanced', 'tasks'], queues: ['advanced', 'queues'], network: ['advanced', 'network'], grants: ['advanced', 'grants'], statistics: ['advanced', 'statistics'] } as const
export function useTaskOptions() {
  return useQuery({ queryKey: advancedKeys.tasks, queryFn: advanced.tasks, retry: false, staleTime: 2000, refetchInterval: 10_000 })
}
export function useQueues() {
  return useQuery({ queryKey: advancedKeys.queues, queryFn: advanced.queues, retry: false, staleTime: 2000, refetchInterval: 5000 })
}
export function useGrants() {
  return useQuery({ queryKey: advancedKeys.grants, queryFn: advanced.grants, retry: false, staleTime: 5000 })
}
