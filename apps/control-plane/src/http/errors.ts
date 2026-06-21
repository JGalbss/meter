//! Shared HTTP error handling: the union of failures a handler can raise, the mapping to JSON
//! responses, and the `handle` wrapper that makes a route total (never rejects).

import type { HttpServerError } from "@effect/platform";
import { HttpServerResponse } from "@effect/platform";
import type { ParseResult } from "effect";
import { Effect, Match } from "effect";

import type { NotFound, RepoError } from "../repository/errors";

/** Every failure a handler can surface. */
export type AppError = RepoError | NotFound | ParseResult.ParseError | HttpServerError.RequestError;

/** A 403 for a caller acting outside its tenant scope (ADR 0007). Returned directly by handlers. */
export const forbidden: HttpServerResponse.HttpServerResponse = HttpServerResponse.unsafeJson(
  { error: "forbidden" },
  { status: 403 },
);

/** Map a typed failure to a clean JSON response. A response is itself a yieldable `Effect`. */
export function errorResponse(
  error: AppError,
): Effect.Effect<HttpServerResponse.HttpServerResponse> {
  return Match.value(error).pipe(
    Match.tag("RepoError", () =>
      HttpServerResponse.unsafeJson({ error: "internal" }, { status: 500 }),
    ),
    Match.tag("NotFound", (notFound) =>
      HttpServerResponse.unsafeJson(
        { error: "not_found", resource: notFound.resource, id: notFound.id },
        { status: 404 },
      ),
    ),
    Match.tag("ParseError", (parse) =>
      HttpServerResponse.unsafeJson({ error: "invalid", detail: parse.message }, { status: 400 }),
    ),
    Match.tag("RequestError", () =>
      HttpServerResponse.unsafeJson({ error: "invalid_request" }, { status: 400 }),
    ),
    Match.orElse(() => HttpServerResponse.unsafeJson({ error: "internal" }, { status: 500 })),
  );
}

/** Run a handler, turning any typed failure into a JSON response so the route never rejects. */
export function handle<R>(
  effect: Effect.Effect<HttpServerResponse.HttpServerResponse, AppError, R>,
): Effect.Effect<HttpServerResponse.HttpServerResponse, never, R> {
  return effect.pipe(Effect.catchAll(errorResponse));
}
