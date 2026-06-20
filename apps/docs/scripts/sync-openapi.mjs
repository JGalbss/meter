// Copy each surface's emitted OpenAPI document into the docs app so the generated API references render
// from the committed contracts. CI re-runs this and fails if a copy drifts from its source, so the
// published references can never fall behind the specs.
//   - control plane: emitted by `pnpm --filter @meter/control-plane openapi:emit`
//   - engine: emitted by the Rust `openapi_spec` test (committed at crates/meter-api/openapi.json)
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, "..", "..", "..");

const surfaces = [
  {
    source: join(repoRoot, "apps", "control-plane", "openapi.json"),
    dest: join(here, "..", "lib", "control-plane-openapi.json"),
  },
  {
    source: join(repoRoot, "crates", "meter-api", "openapi.json"),
    dest: join(here, "..", "lib", "engine-openapi.json"),
  },
];

for (const { source, dest } of surfaces) {
  mkdirSync(dirname(dest), { recursive: true });
  copyFileSync(source, dest);
  console.log(`synced ${source} -> ${dest}`);
}
