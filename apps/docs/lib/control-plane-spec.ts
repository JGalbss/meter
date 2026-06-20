// The control plane's emitted OpenAPI document, bound to the shared typed view in `lib/openapi.ts`.
// Synced from `apps/control-plane/openapi.json` by `scripts/sync-openapi.mjs` and drift-checked in CI.
import type { OpenApiDocument } from "./openapi";

import document from "./control-plane-openapi.json";

export const controlPlaneSpec: OpenApiDocument = document as unknown as OpenApiDocument;
