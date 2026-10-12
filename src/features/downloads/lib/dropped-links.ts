import { pastedLinks } from './pasted-links'

/** Prefer the browser's link payload over its human-readable link title. */
export function droppedLinks(data: Pick<DataTransfer, 'getData'>): string[] {
  const uriList = data.getData('text/uri-list')
  if (uriList.trim()) {
    return pastedLinks(
      uriList
        .split(/\r?\n/)
        .filter((line) => !line.trim().startsWith('#'))
        .join('\n'),
    )
  }
  const firefoxLink = data.getData('text/x-moz-url')
  if (firefoxLink.trim()) return pastedLinks(firefoxLink.split(/\r?\n/)[0])
  return pastedLinks(data.getData('text/plain'))
}
