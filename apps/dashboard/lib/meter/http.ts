//! Shared server-side HTTP helpers for the meter clients (control plane + engine). Every request is
//! bounded by a timeout so a *slow or hung* upstream degrades to a friendly error just like a refused
//! one — the page never hangs waiting on it. Reads return a `Result`; mutations throw.

/** The outcome of a read: data on success, a human-readable error when the upstream was unreachable. */
export type Result<T> =
  | { readonly ok: true; readonly data: T }
  | { readonly ok: false; readonly error: string }

// Per-request timeout (ms). A hung upstream should fail fast, not block a server render indefinitely.
const TIMEOUT_MS = Number.parseInt(
  process.env.METER_HTTP_TIMEOUT_MS ?? "10000",
  10
)

/** Whether an error is an `AbortSignal.timeout` firing (a `TimeoutError` DOMException). */
export function isTimeoutError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "TimeoutError"
}

/** A human-readable message for a failed request, distinguishing a timeout from other failures. */
export function describeError(label: string, error: unknown): string {
  if (isTimeoutError(error)) {
    return `${label} timed out`
  }
  if (error instanceof Error) {
    return error.message
  }
  return `${label} unreachable`
}

function withDefaults(init: RequestInit): RequestInit {
  return { cache: "no-store", ...init, signal: AbortSignal.timeout(TIMEOUT_MS) }
}

/** Perform a request and decode JSON into a `Result` — never throws (a down/slow/erroring upstream
 * becomes `{ ok: false }`). `label` names the upstream for error messages ("control plane" / "engine"). */
export async function getResult<T>(
  label: string,
  url: string,
  init: RequestInit = {}
): Promise<Result<T>> {
  try {
    const response = await fetch(url, withDefaults(init))
    if (!response.ok) {
      return { ok: false, error: `${label} responded ${response.status}` }
    }
    return { ok: true, data: (await response.json()) as T }
  } catch (error) {
    return { ok: false, error: describeError(label, error) }
  }
}

/** Perform a request, throwing on a non-2xx or transport/timeout failure. For mutations, where the
 * caller (a server action) surfaces the thrown error. */
export async function requestOrThrow(
  label: string,
  url: string,
  init: RequestInit = {}
): Promise<Response> {
  const response = await fetch(url, withDefaults(init))
  if (!response.ok) {
    throw new Error(`${label} responded ${response.status}`)
  }
  return response
}
