import { createRouterClient } from '@orpc/server'
import { createTanstackQueryUtils } from '@orpc/tanstack-query'
import { appRouter } from './router'

// No local HTTP server, URL, API key or fetch adapter is necessary.
const client = createRouterClient(appRouter, { context: {} })

export const orpc = createTanstackQueryUtils(client)
