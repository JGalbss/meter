//! Request observability: a request id (propagated from the caller or generated), echoed in the
//! response and attached to every log line emitted while handling the request, plus one structured
//! access-log line per request with method, path, status, and duration.

import { randomUUID } from "node:crypto";

import { HttpMiddleware, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Duration, Effect } from "effect";

const REQUEST_ID = "x-request-id";

function resolveRequestId(headers: Record<string, string | undefined>): string {
  const provided = headers[REQUEST_ID];
  if (provided !== undefined && provided.length > 0) {
    return provided;
  }
  return randomUUID();
}

/** Wrap an HTTP app with request-id propagation and a structured access log. */
export const withObservability = HttpMiddleware.make((httpApp) =>
  Effect.gen(function* () {
    const request = yield* HttpServerRequest.HttpServerRequest;
    const requestId = resolveRequestId(request.headers);
    const [elapsed, response] = yield* Effect.timed(
      httpApp.pipe(Effect.annotateLogs({ requestId })),
    );
    yield* Effect.logInfo("http_request").pipe(
      Effect.annotateLogs({
        requestId,
        method: request.method,
        url: request.url,
        status: response.status,
        durationMs: Duration.toMillis(elapsed),
      }),
    );
    return response.pipe(HttpServerResponse.setHeader(REQUEST_ID, requestId));
  }),
);
