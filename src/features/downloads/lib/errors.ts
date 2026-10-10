import type { ErrorCode } from '@/lib/orpc/schemas'

const messages: Record<ErrorCode, string> = {
  INVALID_URL: 'Enter a valid download URL.',
  UNSUPPORTED_SCHEME: 'Use an HTTPS URL. HTTP is supported only for localhost.',
  PATH_NOT_ALLOWED: 'The file could not be saved. Check the filename and folder permissions.',
  ALREADY_EXISTS: 'A file with this name already exists. Try another filename.',
  DISK_FULL: 'Free some disk space, then retry the download.',
  NETWORK_TIMEOUT: 'The connection timed out. Check your connection and retry.',
  HTTP_UNAUTHORIZED: 'This link requires sign-in. Try a direct download link.',
  HTTP_FORBIDDEN: 'The server denied access. Check the download link.',
  HTTP_RATE_LIMIT: 'The server is busy. Wait a moment before retrying.',
  RESOURCE_CHANGED: 'The file changed on the server. Add a fresh download.',
  RANGE_UNSUPPORTED: 'The server cannot continue this partial download. Add a fresh download.',
  LENGTH_MISMATCH: 'The received file length is incorrect. Add a fresh download.',
  URL_NOT_ALLOWED: 'Use a public HTTPS link. Private addresses and unsafe redirects are blocked.',
  CHECKSUM_MISMATCH: 'SHA-256 verification failed. Obtain a trusted download link or checksum; the file was kept.',
  TASK_NOT_FOUND: 'This download no longer exists. The list has been refreshed.',
  INVALID_TRANSITION: 'The download changed while you were acting. Check its current status.',
  DATA_DIRECTORY_IN_USE: 'Another Circle process is using this data folder. Close it before reopening the app.',
  NETWORK_CONNECTION: 'Could not reach the server. Check your connection and the download host, then retry.',
  NETWORK_INTERRUPTED: 'The connection ended before the download finished. Check your connection and retry.',
  TLS_ERROR: 'The secure connection could not be verified. Check your system clock or contact the site owner.',
  HTTP_NOT_FOUND: 'The file is no longer available. Get a new download link.',
  HTTP_CLIENT_ERROR: 'The server rejected this request. Check the link or obtain a new one.',
  HTTP_SERVER_ERROR: 'The server failed to handle the download. Wait a moment and retry.',
  HTTP_REDIRECT: 'The link redirects too many times or to an invalid location. Use a direct file link.',
  STORAGE_PERMISSION: 'Circle cannot write to this folder. Check its permissions or read-only status.',
  STORAGE_NOT_FOUND: 'The download folder or partial file is missing. Restore it or add a fresh download.',
  STORAGE_ERROR: 'The storage operation failed. Check the drive is connected and working, then retry.',
  INTERNAL: 'Something went wrong. Retry the action or restart the app.',
}
export function downloadErrorMessage(error: unknown) {
  if (error !== null && error !== undefined && typeof error === 'object' && 'code' in error && typeof error.code === 'string' && Object.keys(messages).includes(error.code))
    return messages[error.code as ErrorCode]
  return messages.INTERNAL
}
