import { z } from 'zod'

export const createFormSchema = z.object({
  url: z.string().trim().url('Enter a valid URL.').refine((value) => {
    try {
      const url = new URL(value)
      return url.protocol === 'https:' || (url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]', '::1'].includes(url.hostname))
    }
    catch { return false }
  }, 'Use HTTPS, or HTTP for localhost only.'),
  suggestedName: z.string().trim(),
  startNow: z.boolean(),
})
export type CreateFormValues = z.infer<typeof createFormSchema>
