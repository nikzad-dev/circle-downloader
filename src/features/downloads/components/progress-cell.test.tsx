import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { task } from '@/test/fixtures'
import { ProgressCell } from './progress-cell'

describe('accessible progress', () => {
  it('shows determinate bytes and speed without live byte announcements', () => {
    render(<ProgressCell task={task()} />)
    expect(screen.getByRole('progressbar', { name: 'Progress for file.zip' })).toHaveAttribute('aria-valuenow', '10')
    expect(screen.getByText('20 B/s')).toBeInTheDocument()
  })
  it.each([null, 0])('handles unknown or zero totals without invalid percentages', (totalBytes) => {
    render(<ProgressCell task={task({ totalBytes })} />)
    expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow')
  })
  it('distinguishes verification from completion', () => {
    render(<ProgressCell task={task({ status: 'verifying', downloadedBytes: 1000 })} />)
    expect(screen.getByText('Verifying')).toBeInTheDocument()
  })
})
