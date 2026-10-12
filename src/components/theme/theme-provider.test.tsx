import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ThemeProvider } from './theme-provider'
import { ThemeSelector } from './theme-selector'

let change: () => void
const media = {
  matches: false,
  addEventListener: vi.fn((_event: string, listener: () => void) => {
    change = listener
  }),
  removeEventListener: vi.fn(),
}
beforeEach(() => {
  localStorage.clear()
  media.matches = false
  media.addEventListener.mockClear()
  media.removeEventListener.mockClear()
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn(() => media as unknown as MediaQueryList),
  })
})
describe('appearance integration', () => {
  it('shares the context, follows system changes and cleans up its listener', () => {
    const view = render(
      <ThemeProvider>
        <ThemeSelector />
      </ThemeProvider>,
    )
    expect(screen.getByRole('radio', { name: 'System' })).toBeChecked()
    expect(document.documentElement).toHaveClass('light')
    act(() => {
      media.matches = true
      change()
    })
    expect(document.documentElement).toHaveClass('dark')
    view.unmount()
    expect(media.removeEventListener).toHaveBeenCalledWith('change', change)
  })
  it('persists explicit preference and ignores OS changes in explicit mode', async () => {
    const user = userEvent.setup()
    render(
      <ThemeProvider>
        <ThemeSelector />
      </ThemeProvider>,
    )
    await user.click(screen.getByRole('radio', { name: 'Dark' }))
    expect(localStorage.getItem('vite-ui-theme')).toBe('dark')
    expect(document.documentElement).toHaveClass('dark')
    act(() => {
      media.matches = false
    })
    expect(document.documentElement).toHaveClass('dark')
  })
  it('loads saved preferences and validates corrupted values', () => {
    localStorage.setItem('vite-ui-theme', 'invalid')
    const view = render(
      <ThemeProvider>
        <ThemeSelector />
      </ThemeProvider>,
    )
    expect(screen.getByRole('radio', { name: 'System' })).toBeChecked()
    view.unmount()
    localStorage.setItem('vite-ui-theme', 'dark')
    render(
      <ThemeProvider>
        <ThemeSelector />
      </ThemeProvider>,
    )
    expect(document.documentElement).toHaveClass('dark')
  })
  it('still changes appearance when storage is unavailable', async () => {
    const get = vi
      .spyOn(Storage.prototype, 'getItem')
      .mockImplementation(() => {
        throw new Error('blocked')
      })
    const set = vi
      .spyOn(Storage.prototype, 'setItem')
      .mockImplementation(() => {
        throw new Error('blocked')
      })
    render(
      <ThemeProvider>
        <ThemeSelector />
      </ThemeProvider>,
    )
    await userEvent.setup().click(screen.getByRole('radio', { name: 'Dark' }))
    expect(document.documentElement).toHaveClass('dark')
    get.mockRestore()
    set.mockRestore()
  })
})
