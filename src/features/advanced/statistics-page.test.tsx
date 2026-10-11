import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { StatisticsPage } from './statistics-page'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
function show() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(<QueryClientProvider client={client}><StatisticsPage /></QueryClientProvider>)
}
describe('persisted statistics', () => {
  it('shows an honest empty state without sample graphs', async () => {
    invoke.mockResolvedValue({ transferredBytes: 0, transferMillis: 0, completedAttempts: 0, failedAttempts: 0, days: [] })
    show()
    expect(await screen.findByText(/No recorded transfer attempts yet/)).toBeInTheDocument()
    expect(screen.queryByRole('table')).not.toBeInTheDocument()
    expect(document.querySelector('svg')).toBeNull()
  })
  it('shows actual persisted daily measurements', async () => {
    invoke.mockResolvedValue({ transferredBytes: 2048, transferMillis: 1250, completedAttempts: 1, failedAttempts: 0, days: [{ day: '2026-10-11', transferredBytes: 2048, transferMillis: 1250 }] })
    show()
    expect(await screen.findByRole('table')).toHaveTextContent('2026-10-11')
    expect(screen.getByRole('table')).toHaveTextContent('2 KB')
    expect(screen.getByRole('table')).toHaveTextContent('1.3 s')
  })
})
