import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { task } from '@/test/fixtures'
import { HomeOverviewPage } from './home-overview-page'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@tanstack/react-router', () => ({
  Link: ({
    children,
    ...props
  }: {
    children: React.ReactNode
    to?: string
  }) => <a href={props.to ?? '#'}>{children}</a>,
}))

function show() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return render(
    <QueryClientProvider client={client}>
      <HomeOverviewPage />
    </QueryClientProvider>,
  )
}

describe('HomeOverviewPage', () => {
  it('renders summary counts with rolling numbers', async () => {
    invoke.mockResolvedValue([
      task({ id: 'task-1', status: 'downloading' }),
      task({ id: 'task-2', status: 'completed' }),
      task({ id: 'task-3', status: 'failed' }),
    ])
    show()
    expect(await screen.findByText('Active transfers')).toBeInTheDocument()
    expect(screen.getByText('Completed')).toBeInTheDocument()
    expect(screen.getByText('Needs attention')).toBeInTheDocument()

    const activeCard = screen.getByText('Active transfers').closest('div')
    const completedCard = screen.getByText('Completed').closest('div')
    const attentionCard = screen.getByText('Needs attention').closest('div')

    await waitFor(() => {
      expect(activeCard).toHaveTextContent('1')
      expect(completedCard).toHaveTextContent('1')
      expect(attentionCard).toHaveTextContent('1')
    })
  })
})
