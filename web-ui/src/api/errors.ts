/**
 * API error envelope parsing.
 *
 * Backend error responses follow the shape:
 *   { "error": <localized message>, "code": "<route_level.code>" }
 *
 * `getErrorCode` / `isErrorCode` duck-type on any error that carries the
 * pre-parsed `data` (ky v2 HTTPError) or a `response.json()` body (test
 * doubles alike), so call sites can react to behavior codes without knowing
 * ky's types.
 */

export interface ApiError {
  status: number
  code: string
  message: string
}

interface ErrorWithResponse {
  response?: {
    status: number
    json: () => Promise<unknown>
  }
  data?: unknown
}

/** `code` attached by client.ts's beforeError hook, if any — avoids re-parsing the body. */
function attachedCode(err: unknown): string | null {
  if (typeof err === 'object' && err !== null) {
    const code = (err as { code?: unknown }).code
    if (typeof code === 'string' && code.length > 0) return code
  }
  return null
}

function hasResponse(err: unknown): err is ErrorWithResponse {
  return (
    typeof err === 'object' &&
    err !== null &&
    'response' in err &&
    typeof (err as ErrorWithResponse).response?.json === 'function'
  )
}

async function parseCode(err: unknown): Promise<string | null> {
  const attached = attachedCode(err)
  if (attached !== null) return attached
  if (typeof err === 'object' && err !== null) {
    // ky v2 pre-parses the body into `error.data` and consumes it, so the
    // `response.json()` fallback below throws — check `data` first.
    const data = (err as { data?: unknown }).data
    if (typeof data === 'object' && data !== null) {
      const code = (data as { code?: unknown }).code
      if (typeof code === 'string') return code
    }
  }
  if (!hasResponse(err)) return null
  try {
    const body = (await err.response!.json()) as { code?: unknown }
    return typeof body.code === 'string' ? body.code : null
  } catch {
    // Non-JSON body (HTML error page, network proxy page, ...) — no code.
    return null
  }
}

/** Extract the behavior `code` from an error, or null when unavailable. */
export async function getErrorCode(err: unknown): Promise<string | null> {
  return parseCode(err)
}

/** True when the error's body carries the given behavior `code`. */
export async function isErrorCode(err: unknown, code: string): Promise<boolean> {
  return (await getErrorCode(err)) === code
}
