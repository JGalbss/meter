//! The database as an Effect service. Handlers depend on `Database`; the entrypoint (production) and
//! the tests (PGlite) each provide a concrete [`Db`]. This keeps HTTP handlers driver-agnostic.

import { Context } from "effect";

import type { Db } from "./client";

export class Database extends Context.Tag("Database")<Database, Db>() {}
