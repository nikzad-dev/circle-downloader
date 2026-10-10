import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { expect, it, vi } from 'vitest'
import { task } from '@/test/fixtures'
import { DownloadsTable } from './downloads-table'

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, ...props }: { children: React.ReactNode, to?: string }) => <a href={props.to ?? '#'}>{children}</a>,
}))

it('distinguishes a calculated digest from a matched expected checksum', () => {
  const client = new QueryClient()
  const digest = 'a'.repeat(64)
  render(
    <QueryClientProvider client={client}>
      <DownloadsTable
        showSelection={false}
        tasks={[
          task({ id: 'computed', status: 'completed', actualSha256: digest, checksumResult: 'computed' }),
          task({ id: 'matched', status: 'completed', actualSha256: digest, expectedSha256: digest, checksumResult: 'matched' }),
        ]}
      />
    </QueryClientProvider>,
  )
  expect(screen.getByText('SHA-256 calculated · no expected checksum')).toBeInTheDocument()
  expect(screen.getByText('SHA-256 verified')).toBeInTheDocument()
  expect(screen.getAllByText(digest)).toHaveLength(2)
})
