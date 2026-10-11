import { describe, expect, it } from 'vitest'
import { categoryFromFilename, categoryFromSource, downloadCategories } from './categories'

describe('file categories', () => {
  it('provides the six categories in the requested order', () => {
    expect(downloadCategories).toEqual(['Compressed', 'Documents', 'Music', 'Pictures', 'Programs', 'Videos'])
  })
  it.each([
    ['archive.tar.gz', 'Compressed'],
    ['REPORT.PDF', 'Documents'],
    ['song.flac', 'Music'],
    ['photo.heic', 'Pictures'],
    ['installer.AppImage', 'Programs'],
    ['movie.mkv', 'Videos'],
    ['download', ''],
    ['unknown.custom', ''],
    ['movie.mp4.exe', 'Programs'],
  ])('classifies %s as %s', (filename, category) => {
    expect(categoryFromFilename(filename)).toBe(category)
  })
  it('uses only the URL path, decodes extensions and honors custom filenames', () => {
    expect(categoryFromSource('https://example.com/report%2Epdf?file=movie.mp4#photo.jpg')).toBe('Documents')
    expect(categoryFromSource('https://example.com/download?filename=movie.mp4')).toBe('')
    expect(categoryFromSource('https://example.com/movie.mp4', 'report.pdf')).toBe('Documents')
    expect(categoryFromSource('invalid')).toBe('')
  })
})
