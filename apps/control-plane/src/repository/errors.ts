//! Shared repository error channels. Every repository fails with one of these typed errors so the
//! HTTP layer can map them to responses uniformly (see `http/router.ts`).

import { Data } from "effect";

/** A failure talking to the database. */
export class RepoError extends Data.TaggedError("RepoError")<{ readonly cause: unknown }> {}

/** A requested resource does not exist. Maps to HTTP 404. */
export class NotFound extends Data.TaggedError("NotFound")<{
  readonly resource: string;
  readonly id: string;
}> {}
