import type { ReactNode } from 'react'
import type { Theme } from './theme-context'
import { useCallback, useLayoutEffect, useMemo, useState } from 'react'
import { ThemeContext } from './theme-context'

export function ThemeProvider({
  children,
  defaultTheme = 'system',
  storageKey = 'vite-ui-theme',
}: {
  children: ReactNode
  defaultTheme?: Theme
  storageKey?: string
}) {
  const [selectedTheme, setSelectedTheme] = useState<Theme>(() => {
    try {
      const stored = localStorage.getItem(storageKey)
      return stored === 'light' || stored === 'dark' || stored === 'system'
        ? stored
        : defaultTheme
    } catch {
      return defaultTheme
    }
  })
  useLayoutEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const apply = () => {
      const resolved =
        selectedTheme === 'system'
          ? media.matches
            ? 'dark'
            : 'light'
          : selectedTheme
      document.documentElement.classList.remove('light', 'dark')
      document.documentElement.classList.add(resolved)
      document.documentElement.style.colorScheme = resolved
    }
    apply()
    if (selectedTheme !== 'system') return
    media.addEventListener('change', apply)
    return () => media.removeEventListener('change', apply)
  }, [selectedTheme])
  const setTheme = useCallback(
    (next: Theme) => {
      setSelectedTheme(next)
      try {
        localStorage.setItem(storageKey, next)
      } catch {
        /* Appearance works without storage. */
      }
    },
    [storageKey],
  )
  const value = useMemo(
    () => ({ theme: selectedTheme, setTheme }),
    [selectedTheme, setTheme],
  )
  return <ThemeContext value={value}>{children}</ThemeContext>
}
