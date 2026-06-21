// Generated control-plane API reference. Rendered directly from the committed OpenAPI document
// (`lib/control-plane-openapi.json`, synced from the control plane and drift-checked in CI), so it can
// never fall behind the contract. The hand-written overview lives at /api.
import type { ReactNode } from "react";

import { ApiReference } from "../../../components/api-reference";
import { controlPlaneSpec } from "../../../lib/control-plane-spec";

export const metadata = {
  title: "Control plane API (generated)",
  description: "The control-plane HTTP surface, generated from its OpenAPI 3.1 contract.",
};

export default function ControlPlaneApiReference(): ReactNode {
  return (
    <ApiReference
      title="Control plane API"
      document={controlPlaneSpec}
      intro={
        <p>
          Generated from the control plane's OpenAPI {controlPlaneSpec.openapi} contract (version{" "}
          {controlPlaneSpec.info.version}) — the same document served at{" "}
          <code>GET /openapi.json</code> and used to generate the dashboard's client types. This
          page is rebuilt from that contract, so it cannot drift from the live surface. For the
          engine surface see <a href="/api/engine">the engine API reference</a>, and for a narrative
          overview see <a href="/api">API reference</a>.
        </p>
      }
    />
  );
}
