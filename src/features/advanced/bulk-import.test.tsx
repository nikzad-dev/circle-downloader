import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import { task } from '@/test/fixtures'
import { BulkImportForm } from './bulk-import'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
it('deduplicates imports and includes tags, priority and explicit DST choice', async () => {
  invoke.mockImplementation(async (command) => {
    if (command === 'get_queue_policies')
      return [
        {
          id: 'default',
          name: 'Downloads',
          paused: false,
          maxConcurrent: 3,
          bytesPerSecond: 0,
          taskCount: 0,
        },
      ]
    if (command === 'list_credential_grants') return []
    if (command === 'bulk_import')
      return { created: [task({ id: 'a' }), task({ id: 'b' })], errors: [] }
    throw new Error(`Unexpected command ${command}`)
  })
  const user = userEvent.setup()
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  render(
    <QueryClientProvider client={client}>
      <BulkImportForm />
    </QueryClientProvider>,
  )
  await user.type(
    screen.getByLabelText('Download URLs'),
    'https://example.com/a\nhttps://example.com/a\nhttps://example.com/b',
  )
  await user.click(screen.getByLabelText(/Organize downloads/))
  await user.type(
    screen.getByLabelText('Tags (comma separated)'),
    'papers, review',
  )
  await user.selectOptions(screen.getByLabelText('Priority'), '2')
  await user.click(screen.getByLabelText('Schedule start'))
  fireEvent.change(screen.getByLabelText('Start no earlier than'), {
    target: { value: '2026-11-01T01:30' },
  })
  await user.clear(screen.getByLabelText('IANA time zone'))
  await user.type(screen.getByLabelText('IANA time zone'), 'America/New_York')
  await user.selectOptions(
    screen.getByLabelText('If clocks repeat this time'),
    'later',
  )
  await user.click(screen.getByRole('button', { name: 'Import 2 URLs' }))
  await waitFor(() =>
    expect(screen.getByRole('status')).toHaveTextContent(
      'Imported 2 downloads.',
    ),
  )
  const args = invoke.mock.calls.find(
    (call) => call[0] === 'bulk_import',
  )?.[1] as { input: { urls: string[]; options: unknown } }
  const input = args.input
  expect(input.urls).toEqual(['https://example.com/a', 'https://example.com/b'])
  expect(input.options).toMatchObject({
    tags: ['papers', 'review'],
    priority: 2,
    schedule: {
      localTime: '2026-11-01T01:30',
      timeZone: 'America/New_York',
      ambiguity: 'later',
    },
  })
  expect(screen.getByLabelText('Download URLs')).toHaveValue('')
})
