/** Treat clipboard text as a URL list, never scrape links out of arbitrary prose. */
export function pastedLinks(text: string): string[] {
  const tokens = text.trim().split(/\s+/).filter(Boolean)
  if (tokens.length === 0)
    return []
  if (!tokens.every((token) => {
    try {
      return ['http:', 'https:'].includes(new URL(token).protocol) && /^https?:\/\//i.test(token)
    }
    catch { return false }
  })) {
    return []
  }
  return [...new Set(tokens)]
}
