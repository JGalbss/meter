//! API-key authentication middleware. When enabled, every request except `/health` must carry a
//! valid `Authorization: Bearer <token>`. Applied by wrapping the router before it is served.

import { HttpMiddleware, HttpServerRequest, HttpServerResponse } from "@effect/platform";
import { Effect, Either } from "effect";

import { verifyApiKey } from "../api-keys/repository";
import type { Db } from "../db/client";
import { requiredRole, roleSatisfies } from "./rbac";

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
const forbidden = HttpServerResponse.unsafeJson({ error: "forbidden" }, { status: 403 });
const serverError = HttpServerResponse.unsafeJson({ error: "internal" }, { status: 500 });

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
      // `verifyApiKey` returns `null` for an unknown/invalid key and only *fails* on a real database
      // error — surface that as a logged 500 rather than silently masking it as a 401.
      const verified = yield* Effect.either(verifyApiKey(db, token));
      if (Either.isLeft(verified)) {
        yield* Effect.logError("auth: api-key verification failed", verified.left);
        return serverError;
      }
      const principal = verified.right;
      if (principal === null) {
        return unauthorized;
      }
      if (!roleSatisfies(principal.role, requiredRole(request.method, request.url))) {
        return forbidden;
      }
      return yield* httpApp;
    }),
  );
}
