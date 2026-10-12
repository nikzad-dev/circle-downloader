import { describe, expect, it } from 'vitest'
import { task } from '@/test/fixtures'
import { filterAndSortTasks } from './task-query'

function many(count: number) {
  return Array.from({ length: count }, (_, index) =>
    task({
      id: `t-${index}`,
      fileName: `file-${String(index).padStart(4, '0')}.bin`,
      status:
        index % 3 === 0
          ? 'failed'
          : index % 2 === 0
            ? 'completed'
            : 'downloading',
      createdAt: `2026-10-10T${String(index % 24).padStart(2, '0')}:00:00Z`,
      url: `https://example.com/${index}`,
    }),
  )
}

describe('filterAndSortTasks', () => {
  it('filters by status and query without mutating input order of the cache source', () => {
    const source = [
      task({
        id: 'a',
        fileName: 'alpha.zip',
        status: 'failed',
        createdAt: '2026-10-10T01:00:00Z',
      }),
      task({
        id: 'b',
        fileName: 'beta.zip',
        status: 'completed',
        createdAt: '2026-10-10T02:00:00Z',
      }),
      task({
        id: 'c',
        fileName: 'alpha-extra.zip',
        status: 'failed',
        createdAt: '2026-10-10T03:00:00Z',
      }),
    ]
    const frozen = source.map((row) => row.id)
    const rows = filterAndSortTasks(source, {
      q: 'alpha',
      status: 'failed',
      sort: 'created_desc',
    })
    expect(rows.map((row) => row.id)).toEqual(['c', 'a'])
    expect(source.map((row) => row.id)).toEqual(frozen)
  })

  it('sorts by name and status stably for 1000 tasks', () => {
    const source = many(1000)
    const byName = filterAndSortTasks(source, {
      sort: 'name_asc',
      status: 'all',
    })
    expect(byName).toHaveLength(1000)
    expect(byName[0]?.fileName).toBe('file-0000.bin')
    expect(byName[999]?.fileName).toBe('file-0999.bin')
    const failed = filterAndSortTasks(source, {
      status: 'failed',
      sort: 'created_asc',
    })
    expect(failed.every((row) => row.status === 'failed')).toBe(true)
    expect(failed.length).toBeGreaterThan(300)
  })

  it('progress-style identity: re-filtering the same tasks yields the same ids in sort order', () => {
    const source = many(50)
    const first = filterAndSortTasks(source, { sort: 'status_asc' }).map(
      (row) => row.id,
    )
    const patched = source.map((row) =>
      row.id === 't-1'
        ? { ...row, downloadedBytes: 999, revision: row.revision + 1 }
        : row,
    )
    const second = filterAndSortTasks(patched, { sort: 'status_asc' }).map(
      (row) => row.id,
    )
    expect(second).toEqual(first)
  })
})

it('combines queue/category/tag filters with transfer status', () => {
  const source = [
    task({ id: 'a', status: 'paused' }),
    task({ id: 'b', status: 'completed' }),
    task({ id: 'c', status: 'paused' }),
  ]
  const controls = new Map(
    source.map((row) => [
      row.id,
      {
        id: row.id,
        queueId: 'research',
        category: 'Papers',
        tags: row.id === 'c' ? ['other'] : ['review'],
        priority: 0,
        schedule: null,
        scheduledAt: null,
        credentialId: null,
      },
    ]),
  )
  expect(
    filterAndSortTasks(
      source,
      {
        queueId: 'research',
        category: 'Papers',
        tag: 'review',
        status: 'paused',
      },
      controls,
    ).map((row) => row.id),
  ).toEqual(['a'])
})

it.skipIf(process.env.CIRCLE_BENCHMARKS !== '1')(
  'large history repeated benchmark',
  () => {
    const source = many(20_000)
    const samples: number[] = []
    for (let round = 0; round < 8; round++) {
      const start = performance.now()
      const result = filterAndSortTasks(source, {
        status: 'failed',
        sort: 'name_asc',
      })
      expect(result).toHaveLength(6667)
      const elapsed = performance.now() - start
      if (round > 0) samples.push(elapsed)
    }
    const sorted = [...samples].sort((a, b) => a - b)
    const mean = samples.reduce((sum, v) => sum + v, 0) / samples.length
    const variance =
      samples.reduce((sum, v) => sum + (v - mean) ** 2, 0) /
      (samples.length - 1)
    console.warn(
      JSON.stringify({
        benchmark: 'filter_sort_20000',
        samplesMs: samples,
        medianMs: sorted[3],
        sampleVarianceMs2: variance,
      }),
    )
  },
)

it('filters automatic categories from filenames while respecting explicit overrides', () => {
  const source = [
    task({ id: 'pdf', fileName: 'report.PDF' }),
    task({ id: 'mp3', fileName: 'song.mp3' }),
    task({ id: 'manual', fileName: 'movie.mp4' }),
  ]
  const controls = new Map([
    [
      'manual',
      {
        id: 'manual',
        queueId: 'default',
        category: 'Documents',
        tags: [],
        priority: 0,
        schedule: null,
        scheduledAt: null,
        credentialId: null,
      },
    ],
  ])
  expect(
    filterAndSortTasks(source, { category: 'Documents' }, controls).map(
      (row) => row.id,
    ),
  ).toEqual(['manual', 'pdf'])
})
