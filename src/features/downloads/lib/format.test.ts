import { describe, expect, it } from 'vitest'
import { createFormSchema } from './create-form'
import { formatBytes, formatDate, formatSpeed, taskActions } from './format'

describe('download presentation', () => {
  it('formats unknown, zero, binary units and speed', () => {
    expect(formatBytes(null)).toBe('Unknown size')
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(Number.MAX_SAFE_INTEGER)).toContain('PB')
    expect(formatSpeed(0)).toBe('—')
    expect(formatSpeed(1024)).toBe('1 KB/s')
  })
  it('formats dates cleanly and handles invalid dates gracefully', () => {
    expect(formatDate(null)).toBe('—')
    expect(formatDate('')).toBe('—')
    expect(formatDate('invalid-date')).toBe('—')
    expect(formatDate('2026-10-10T10:00:00Z')).toContain('2026')
  })
  it('limits actions to supported transitions', () => {
    expect(taskActions('verifying')).toEqual({ pause: false, resume: false, cancel: true, retry: false })
    expect(taskActions('completed')).toEqual({ pause: false, resume: false, cancel: false, retry: false })
    expect(taskActions('paused').resume).toBe(true)
    expect(taskActions('failed').retry).toBe(true)
  })
  it.each(['https://example.com/a', 'http://localhost/a', 'http://127.0.0.1/a', 'http://[::1]/a'])('accepts supported URL %s', (url) => {
    expect(createFormSchema.parse({ url: ` ${url} `, suggestedName: ' name.zip ', destinationId: 'default', startNow: true }).suggestedName).toBe('name.zip')
  })
  it.each(['bad', 'http://example.com/a', 'ftp://example.com/a'])('rejects unsupported URL %s', (url) => {
    expect(createFormSchema.safeParse({ url, suggestedName: '', destinationId: 'default', startNow: true }).success).toBe(false)
  })
})
