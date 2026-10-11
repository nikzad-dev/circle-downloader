import extensions from './category-extensions.json'

export const downloadCategories = Object.keys(extensions)
export function categoryFromFilename(filename: string): string {
  try {
    filename = decodeURIComponent(filename)
  }
  catch { /* Keep a literal filename when its percent escapes are invalid. */ }
  const extension = filename.split('.').length > 1 ? filename.split('.').pop()?.toLowerCase() : undefined
  return extension !== undefined && extension !== '' ? Object.entries(extensions).find(([, values]) => values.includes(extension))?.[0] ?? '' : ''
}
export function categoryFromSource(url: string, customFilename = ''): string {
  if (customFilename.trim())
    return categoryFromFilename(customFilename.trim())
  try {
    const pathname = new URL(url).pathname
    return categoryFromFilename(pathname.slice(pathname.lastIndexOf('/') + 1))
  }
  catch { return '' }
}

export function resolvedCategory(filename: string, options?: { category: string, resolvedCategory?: string }): string {
  return (options?.category ?? '') || (options?.resolvedCategory ?? '') || categoryFromFilename(filename)
}
