/** English UI catalog — structured for future locales without a runtime i18n library. */
export const strings = {
  appName: 'Circle',
  addDownload: 'Add download',
  downloads: 'Downloads',
  settings: 'Settings',
  searchPlaceholder: 'File name or URL',
  clearFilters: 'Clear filters',
  noFilterMatches: 'No downloads match these filters.',
  waitingForSlot: 'Waiting for a free download slot',
  collisionPolicy: 'Circle never replaces an existing file. A unique name is chosen instead.',
  quitPolicy: 'Closing the window hides Circle to the tray. Use Quit to stop transfers and exit.',
  autoStart: 'Start downloads immediately when added',
  concurrency: 'Maximum concurrent downloads',
  connectionsPerDownload: 'Connections per download',
  connectionsPerDownloadHint:
    'Optional acceleration (1–4) for files of at least 8 MiB. Uses multiple connections only with compatible byte ranges; otherwise Circle stays on one connection. Applies when a transfer starts or resumes.',
  transferModeSingle: 'Single connection',
  transferModeSegmented: 'Segmented (multi-connection)',
  retriesReadonly: 'Automatic retries: up to 5',
  perHostReadonly: 'Per-host limit: 2',
  addFolder: 'Add folder…',
  openFile: 'Open file',
  revealFolder: 'Reveal folder',
  shortcutAdd: 'Add download',
  interruptedHint: 'Retry to resume safely when validators still match. Add a fresh download when resume is unsafe.',
} as const

export function appLocale(): string {
  if (typeof navigator !== 'undefined' && navigator.language)
    return navigator.language
  return 'en'
}
