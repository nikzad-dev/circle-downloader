import { describe, expect, it } from 'vitest'
import { droppedLinks } from './dropped-links'
const data = (values: Record<string, string>) => ({
  getData: (type: string) => values[type] ?? '',
})
describe('dropped links', () => {
  it('prefers URI lists, ignores comments, deduplicates and preserves signed URLs', () => {
    const url = 'https://example.com/archive.zip?token=a%2Bb&x=1'
    expect(
      droppedLinks(
        data({
          'text/uri-list': `# Browser link\r\n${url}\r\n${url}\r\nhttps://example.com/b`,
          'text/plain': 'Download archive',
        }),
      ),
    ).toEqual([url, 'https://example.com/b'])
  })
  it('handles Firefox URL/title payloads and plain-text URL lists', () => {
    expect(
      droppedLinks(
        data({ 'text/x-moz-url': 'https://example.com/a\nA download' }),
      ),
    ).toEqual(['https://example.com/a'])
    expect(
      droppedLinks(
        data({ 'text/plain': 'https://example.com/a https://example.com/b' }),
      ),
    ).toHaveLength(2)
  })
  it.each([
    'javascript:alert(1)',
    'file:///tmp/example.zip',
    'See https://example.com/a',
    'https://example.com/a\ninvalid',
  ])('rejects %s', (value) => {
    expect(
      droppedLinks(
        data({
          'text/uri-list': value,
          'text/plain': 'https://example.com/fallback',
        }),
      ),
    ).toEqual([])
  })
})
