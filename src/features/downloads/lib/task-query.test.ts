import { describe, expect, it } from 'vitest'
import { task } from '@/test/fixtures'
import { filterAndSortTasks } from './task-query'

function many(count: number) {
  return Array.from({ length: count }, (_, index) =>
    task({
      id: `t-${index}`,
      fileName: `file-${String(index).padStart(4, '0')}.bin`,
      status: index % 3 === 0 ? 'failed' : index % 2 === 0 ? 'completed' : 'downloading',
      createdAt: `2026-10-10T${String(index % 24).padStart(2, '0')}:00:00Z`,
      url: `https://example.com/${index}`,
    }))
}

describe('filterAndSortTasks', () => {
  it('filters by status and query without mutating input order of the cache source', () => {
    const source = [
      task({ id: 'a', fileName: 'alpha.zip', status: 'failed', createdAt: '2026-10-10T01:00:00Z' }),
      task({ id: 'b', fileName: 'beta.zip', status: 'completed', createdAt: '2026-10-10T02:00:00Z' }),
      task({ id: 'c', fileName: 'alpha-extra.zip', status: 'failed', createdAt: '2026-10-10T03:00:00Z' }),
    ]
    const frozen = source.map(row => row.id)
    const rows = filterAndSortTasks(source, { q: 'alpha', status: 'failed', sort: 'created_desc' })
    expect(rows.map(row => row.id)).toEqual(['c', 'a'])
    expect(source.map(row => row.id)).toEqual(frozen)
  })

  it('sorts by name and status stably for 1000 tasks', () => {
    const source = many(1000)
    const byName = filterAndSortTasks(source, { sort: 'name_asc', status: 'all' })
    expect(byName).toHaveLength(1000)
    expect(byName[0]?.fileName).toBe('file-0000.bin')
    expect(byName[999]?.fileName).toBe('file-0999.bin')
    const failed = filterAndSortTasks(source, { status: 'failed', sort: 'created_asc' })
    expect(failed.every(row => row.status === 'failed')).toBe(true)
    expect(failed.length).toBeGreaterThan(300)
  })

  it('progress-style identity: re-filtering the same tasks yields the same ids in sort order', () => {
    const source = many(50)
    const first = filterAndSortTasks(source, { sort: 'status_asc' }).map(row => row.id)
    const patched = source.map(row =>
      row.id === 't-1' ? { ...row, downloadedBytes: 999, revision: row.revision + 1 } : row,
    )
    const second = filterAndSortTasks(patched, { sort: 'status_asc' }).map(row => row.id)
    expect(second).toEqual(first)
  })
})
