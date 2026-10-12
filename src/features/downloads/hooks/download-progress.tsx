import type { ReactNode } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { subscribeProgress } from '@/lib/tauri/progress'
import { ProgressContext } from './progress-context'
import { createProgressController } from './progress-controller'

export function DownloadProgressProvider({
  children,
}: {
  children: ReactNode
}) {
  const client = useQueryClient()
  const [unavailable, setUnavailable] = useState(false)
  const [attempt, setAttempt] = useState(0)
  useEffect(() => {
    let alive = true
    let retries = 0
    let timer: ReturnType<typeof setTimeout> | undefined
    let subscription: ReturnType<typeof subscribeProgress> | undefined
    const controller = createProgressController(client, () => {
      if (alive) setUnavailable(true)
    })
    const connect = () => {
      if (!alive) return
      subscription = subscribeProgress(
        (event) => controller.receive(event),
        () => {
          if (alive) {
            setUnavailable(true)
            controller.reconcile()
          }
        },
      )
      void subscription.ready
        .then(() => {
          if (alive) {
            setUnavailable(false)
            controller.reconcile()
          }
        })
        .catch(() => {
          if (!alive) return
          setUnavailable(true)
          if (retries++ < 2) timer = setTimeout(connect, 1000 * retries)
        })
    }
    connect()
    const recover = () => {
      if (alive) {
        controller.reconcile()
        setAttempt((value) => value + 1)
      }
    }
    window.addEventListener('online', recover)
    return () => {
      alive = false
      clearTimeout(timer)
      controller.dispose()
      window.removeEventListener('online', recover)
      void subscription?.dispose().catch(() => {})
    }
  }, [client, attempt])
  return (
    <ProgressContext
      value={{ unavailable, reconnect: () => setAttempt((value) => value + 1) }}
    >
      {children}
    </ProgressContext>
  )
}
