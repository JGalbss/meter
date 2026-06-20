// The engine's emitted OpenAPI document, bound to the shared typed view in `lib/openapi.ts`. Synced
// from `crates/meter-api/openapi.json` (the Rust `openapi_spec` drift gate's committed artifact) by
// `scripts/sync-openapi.mjs` and drift-checked in CI.
import type { OpenApiDocument } from "./openapi";

import document from "./engine-openapi.json";

export const engineSpec: OpenApiDocument = document as unknown as OpenApiDocument;
