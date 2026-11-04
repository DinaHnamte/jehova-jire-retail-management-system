/** An error from the API, using the standard `{ error: { code, message, details, request_id } }` shape. */
export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
    public readonly details: unknown = {},
    public readonly requestId?: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

/** A short message suitable for showing to staff. */
export function friendlyMessage(err: unknown): string {
  if (!(err instanceof ApiError)) return 'Something went wrong. Please try again.'
  switch (err.code) {
    case 'INVALID_CREDENTIALS':
      return 'Wrong username or password.'
    case 'RATE_LIMITED':
      return 'Too many attempts. Wait 15 minutes and try again.'
    case 'NETWORK_ERROR':
      return "Can't reach the server. Check the internet connection and try again."
    case 'INTERNAL':
      return `Something went wrong on the server.${err.requestId ? ` Reference: ${err.requestId}` : ''}`
    default:
      return err.message
  }
}
