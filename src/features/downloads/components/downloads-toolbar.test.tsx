import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import { DownloadsToolbar } from './downloads-toolbar'

it('shows bulk controls only for selected files and allows clearing selection', async () => {
  const user = userEvent.setup()
  const clear = vi.fn()
  const bulk = vi.fn()
  const props = {
    search: {},
    onChange: vi.fn(),
    selectedCount: 0,
    onBulk: bulk,
    bulkPending: false,
    onClearSelection: clear,
  }
  const { rerender } = render(<DownloadsToolbar {...props} />)
  expect(
    screen.queryByRole('toolbar', { name: 'Bulk download actions' }),
  ).not.toBeInTheDocument()
  rerender(
    <DownloadsToolbar
      {...props}
      selectedCount={2}
      availableActions={['pause', 'remove']}
    />,
  )
  expect(screen.getByText('2 selected')).toBeInTheDocument()
  expect(
    screen.queryByRole('button', { name: 'Resume' }),
  ).not.toBeInTheDocument()
  await user.click(screen.getByRole('button', { name: 'Pause' }))
  expect(bulk).toHaveBeenCalledWith('pause')
  await user.click(screen.getByRole('button', { name: 'Clear selection' }))
  expect(clear).toHaveBeenCalledOnce()
})

it('clears filters without changing the selected section or sort', async () => {
  const user = userEvent.setup()
  const onChange = vi.fn()
  const search = {
    q: 'report',
    category: 'Documents',
    status: 'paused' as const,
    sort: 'name_asc' as const,
  }
  render(
    <DownloadsToolbar
      search={search}
      onChange={onChange}
      selectedCount={0}
      onBulk={vi.fn()}
      bulkPending={false}
    />,
  )
  await user.click(screen.getByRole('button', { name: 'Clear filters' }))
  expect(onChange).toHaveBeenCalledWith({
    ...search,
    q: undefined,
    category: undefined,
    tag: undefined,
    queueId: undefined,
  })
})
it('shows queue names and clears only the requested filter', async () => {
  const user = userEvent.setup()
  const onChange = vi.fn()
  const search = { queueId: 'queue-id', tag: 'work', status: 'active' as const }
  render(
    <DownloadsToolbar
      search={search}
      queueName="Work files"
      onChange={onChange}
      selectedCount={0}
      onBulk={vi.fn()}
      bulkPending={false}
    />,
  )
  await user.click(
    screen.getByRole('button', { name: 'Remove Queue: Work files' }),
  )
  expect(onChange).toHaveBeenCalledWith({ ...search, queueId: undefined })
})
it('sorts from the menu while preserving search and section', async () => {
  const user = userEvent.setup()
  const onChange = vi.fn()
  render(
    <DownloadsToolbar
      search={{ q: 'zip', status: 'completed' }}
      onChange={onChange}
      selectedCount={0}
      onBulk={vi.fn()}
      bulkPending={false}
    />,
  )
  await user.click(screen.getByRole('button', { name: 'Sort: Oldest first' }))
  await user.click(
    await screen.findByRole('menuitemradio', { name: 'Newest first' }),
  )
  expect(onChange).toHaveBeenCalledWith({
    q: 'zip',
    status: 'completed',
    sort: 'created_desc',
  })
})
