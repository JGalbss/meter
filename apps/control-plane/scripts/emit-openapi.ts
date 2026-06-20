//! Write the control-plane OpenAPI document to `openapi.json` — the checked-in, codegen-ready spec.
//! Regenerate with `pnpm --filter @meter/control-plane run openapi:emit`; an `openapi.test.ts` case
//! fails if the committed file drifts from the served document.

import { writeFileSync } from "node:fs";

import { openApiDocument } from "../src/http/openapi";

writeFileSync("openapi.json", `${JSON.stringify(openApiDocument, null, 2)}\n`);
console.info("wrote openapi.json");
