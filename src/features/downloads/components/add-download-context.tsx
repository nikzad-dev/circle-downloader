import { createContext, use } from 'react'

export const AddDownloadContext = createContext<(() => void) | null>(null)
export function useAddDownload() {
  const open = use(AddDownloadContext)
  if (!open)
    throw new Error('Add Download must be used inside AppShell')
  return open
}
