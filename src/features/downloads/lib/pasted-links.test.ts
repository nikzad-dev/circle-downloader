import { describe, expect, it } from 'vitest'
import { pastedLinks } from './pasted-links'

describe('pasted download links', () => {
  it('accepts a single link without changing signed parameters', () => {
    expect(
      pastedLinks('  https://example.com/file.zip?token=a%2Bb&x=1  '),
    ).toEqual(['https://example.com/file.zip?token=a%2Bb&x=1'])
  })
  it('splits newline, CRLF, spaces and tabs and removes exact duplicates in order', () => {
    expect(
      pastedLinks(
        'https://example.com/a\r\nhttps://example.com/b\t https://example.com/a',
      ),
    ).toEqual(['https://example.com/a', 'https://example.com/b'])
  })
  it.each([
    '',
    'ordinary clipboard text',
    'See https://example.com/file',
    'https://example.com/a\ninvalid',
    'file:///tmp/file',
    'javascript:alert(1)',
  ])('does not treat %s as a download list', (value) => {
    expect(pastedLinks(value)).toEqual([])
  })
})
