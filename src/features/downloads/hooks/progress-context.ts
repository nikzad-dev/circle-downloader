import { createContext, use } from 'react'

export const ProgressContext = createContext({ unavailable: false, reconnect: () => {} })
export function useProgressConnection() {
  return use(ProgressContext)
}
