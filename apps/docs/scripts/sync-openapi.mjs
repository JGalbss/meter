// Copy the control plane's emitted OpenAPI document into the docs app so the generated API reference
// renders from the committed contract. CI re-runs this and fails if the copy drifts from the source,
// so the published reference can never fall behind the spec.
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const source = join(here, "..", "..", "control-plane", "openapi.json");
const dest = join(here, "..", "lib", "control-plane-openapi.json");

mkdirSync(dirname(dest), { recursive: true });
copyFileSync(source, dest);
console.log(`synced ${source} -> ${dest}`);
