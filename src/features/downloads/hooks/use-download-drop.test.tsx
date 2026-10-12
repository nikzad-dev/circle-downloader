import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { useDownloadDrop } from './use-download-drop'
const { open, notify } = vi.hoisted(() => ({ open: vi.fn(), notify: vi.fn() }))
vi.mock('../components/download-window-api', () => ({
  openDownloadWindow: open,
}))
vi.mock('@/components/ui/toast', () => ({ toast: { add: notify } }))
function Harness() {
  const dragging = useDownloadDrop()
  return (
    <main>
      <div data-testid="area">
        <span data-testid="child">Downloads</span>
      </div>
      <input aria-label="URL" />
      <div contentEditable suppressContentEditableWarning data-testid="editor">
        Edit
      </div>
      <div role="dialog" aria-label="Options" />
      {dragging && <p role="status">Drop links</p>}
    </main>
  )
}
const transfer = (
  values: Record<string, string>,
  types = Object.keys(values),
) => ({
  types,
  dropEffect: 'none',
  getData: (type: string) => values[type] ?? '',
})
beforeEach(() => {
  open.mockReset()
  notify.mockReset()
})
it('shows a stable target across nested drag events, sets copy feedback and opens a link', () => {
  render(<Harness />)
  const area = screen.getByTestId('area'),
    child = screen.getByTestId('child')
  const dataTransfer = transfer({
    'text/uri-list': 'https://example.com/file.zip',
    'text/plain': 'Download file',
  })
  fireEvent.dragEnter(area, { dataTransfer })
  fireEvent.dragEnter(child, { dataTransfer })
  fireEvent.dragLeave(child, { dataTransfer })
  expect(screen.getByRole('status')).toBeInTheDocument()
  fireEvent.dragOver(area, { dataTransfer })
  expect(dataTransfer.dropEffect).toBe('copy')
  expect(fireEvent.drop(area, { dataTransfer })).toBe(false)
  expect(open).toHaveBeenCalledWith(
    'https://example.com/file.zip',
    false,
    undefined,
  )
  expect(screen.queryByRole('status')).not.toBeInTheDocument()
})
it('opens deduplicated multiple links in bulk mode and enforces limits', () => {
  render(<Harness />)
  const area = screen.getByTestId('area')
  fireEvent.drop(area, {
    dataTransfer: transfer({
      'text/plain':
        'https://example.com/a\nhttps://example.com/b\nhttps://example.com/a',
    }),
  })
  expect(open).toHaveBeenCalledWith('', true, [
    'https://example.com/a',
    'https://example.com/b',
  ])
  open.mockClear()
  for (const value of [
    Array.from({ length: 201 }, (_, i) => `https://example.com/${i}`).join(
      '\n',
    ),
    `https://example.com/${'a'.repeat(8192)}`,
  ])
    fireEvent.drop(area, { dataTransfer: transfer({ 'text/plain': value }) })
  expect(open).not.toHaveBeenCalled()
  expect(notify).toHaveBeenCalledTimes(2)
})
it('prevents invalid and local-file drops from navigating and gives actionable feedback', () => {
  render(<Harness />)
  const area = screen.getByTestId('area')
  for (const dataTransfer of [
    transfer({ 'text/plain': 'javascript:alert(1)' }),
    transfer({ 'text/plain': 'https://example.com/a' }, [
      'Files',
      'text/plain',
    ]),
  ]) {
    fireEvent.dragOver(area, { dataTransfer })
    expect(fireEvent.drop(area, { dataTransfer })).toBe(false)
  }
  expect(open).not.toHaveBeenCalled()
  expect(notify).toHaveBeenCalledTimes(2)
})
it('preserves editing and internal drags', () => {
  render(<Harness />)
  const dataTransfer = transfer({ 'text/plain': 'https://example.com/file' })
  for (const target of [
    screen.getByRole('textbox', { name: 'URL' }),
    screen.getByTestId('editor'),
    screen.getByRole('dialog'),
  ]) {
    fireEvent.dragEnter(target, { dataTransfer })
    expect(fireEvent.drop(target, { dataTransfer })).toBe(true)
  }
  fireEvent.dragStart(screen.getByTestId('child'), { dataTransfer })
  fireEvent.dragEnter(screen.getByTestId('area'), { dataTransfer })
  fireEvent.drop(screen.getByTestId('area'), { dataTransfer })
  expect(open).not.toHaveBeenCalled()
  expect(screen.queryByRole('status')).not.toBeInTheDocument()
})
it('clears feedback on window exit, Escape, blur and drag cancellation', () => {
  render(<Harness />)
  const area = screen.getByTestId('area')
  const dataTransfer = transfer({ 'text/plain': 'https://example.com/file' })
  for (const cancel of [
    () => fireEvent.dragLeave(area),
    () => fireEvent.keyDown(window, { key: 'Escape' }),
    () => fireEvent.blur(window),
    () => fireEvent.dragEnd(window),
  ]) {
    fireEvent.dragEnter(area, { dataTransfer })
    expect(screen.getByRole('status')).toBeInTheDocument()
    cancel()
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
  }
})
