//! API-key authentication middleware. When enabled, every request except `/health` must carry a
//! valid `Authorization: Bearer <token>`. Applied by wrapping the router before it is served.

import { HttpMiddleware, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Effect } from "effect";

import { verifyApiKey } from "../api-keys/repository";
import type { Db } from "../db/client";

const BEARER = "bearer ";

function isExempt(url: string): boolean {
  return url === "/health" || url.startsWith("/health?");
}

function bearerToken(headers: Record<string, string | undefined>): string | null {
  const header = headers.authorization;
  if (header === undefined) {
    return null;
  }
  if (!header.toLowerCase().startsWith(BEARER)) {
    return null;
  }
  const token = header.slice(BEARER.length).trim();
  if (token.length === 0) {
    return null;
  }
  return token;
}

const unauthorized = HttpServerResponse.unsafeJson({ error: "unauthorized" }, { status: 401 });

/** Require a valid API key on every request except `/health`. When `enabled` is false, all requests
 * pass through (useful for local/dev and tests). */
export function requireApiKey(db: Db, enabled: boolean) {
  return HttpMiddleware.make((httpApp) =>
    Effect.gen(function* () {
      if (!enabled) {
        return yield* httpApp;
      }
      const request = yield* HttpServerRequest.HttpServerRequest;
      if (isExempt(request.url)) {
        return yield* httpApp;
      }
      const token = bearerToken(request.headers);
      if (token === null) {
        return unauthorized;
      }
      const orgId = yield* verifyApiKey(db, token).pipe(
        Effect.catchAll(() => Effect.succeed(null)),
      );
      if (orgId === null) {
        return unauthorized;
      }
      return yield* httpApp;
    }),
  );
}
