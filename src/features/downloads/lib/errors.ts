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
  CHECKSUM_MISMATCH: 'The downloaded file is incomplete. Retry the download.',
  TASK_NOT_FOUND: 'This download no longer exists. The list has been refreshed.',
  INVALID_TRANSITION: 'The download changed while you were acting. Check its current status.',
  INTERNAL: 'Something went wrong. Retry the action or restart the app.',
}
export function downloadErrorMessage(error: unknown) {
  if (error !== null && error !== undefined && typeof error === 'object' && 'code' in error && typeof error.code === 'string' && Object.keys(messages).includes(error.code))
    return messages[error.code as ErrorCode]
  return messages.INTERNAL
}
