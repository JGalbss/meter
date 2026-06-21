// Generated engine API reference. Rendered directly from the committed OpenAPI document
// (`lib/engine-openapi.json`, synced from `crates/meter-api/openapi.json` and drift-checked in CI), so
// it can never fall behind the engine's HTTP surface. The hand-written overview lives at /api.
import type { ReactNode } from "react";

import { ApiReference } from "../../../components/api-reference";
import { engineSpec } from "../../../lib/engine-spec";

export const metadata = {
  title: "Engine API (generated)",
  description:
    "The Rust engine's HTTP surface — money-truth and usage — generated from its OpenAPI contract.",
};

export default function EngineApiReference(): ReactNode {
  return (
    <ApiReference
      title="Engine API"
      document={engineSpec}
      intro={
        <p>
          The Rust engine's HTTP surface — money-truth, pricing, enforcement, events, and usage.
          Generated from the engine's OpenAPI {engineSpec.openapi} contract (version{" "}
          {engineSpec.info.version}) — the same document served at <code>GET /openapi.json</code>{" "}
          and the source of truth for engine SDK codegen. This page is rebuilt from that committed
          contract, so it cannot drift from the live surface. For the configuration surface see{" "}
          <a href="/api/control-plane">the control plane API reference</a>, and for a narrative
          overview see <a href="/api">API reference</a>.
        </p>
      }
    />
  );
}
