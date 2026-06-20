// Generated control-plane API reference. Rendered directly from the committed OpenAPI document
// (`lib/control-plane-openapi.json`, synced from the control plane and drift-checked in CI), so it can
// never fall behind the contract. The hand-written overview lives at /api.
import type { ReactNode } from "react";

import {
  type JsonSchema,
  type Operation,
  type Parameter,
  openapi,
  operationsByTag,
  requestSchema,
  schemaLabel,
  schemaLink,
  successResponses,
} from "../../../lib/openapi";

export const metadata = {
  title: "Control plane API (generated)",
  description: "The control-plane HTTP surface, generated from its OpenAPI 3.1 contract.",
};

/** A schema label, deep-linked to its definition when it names a component schema. */
function SchemaRef({ schema }: { schema: JsonSchema | undefined }): ReactNode {
  const link = schemaLink(schema);
  const label = schemaLabel(schema);
  if (link === null) {
    return <code>{label}</code>;
  }
  return (
    <a href={`#schema-${link}`}>
      <code>{label}</code>
    </a>
  );
}

function Parameters({ parameters }: { parameters: readonly Parameter[] }): ReactNode {
  return (
    <table>
      <thead>
        <tr>
          <th>Parameter</th>
          <th>In</th>
          <th>Required</th>
          <th>Type</th>
        </tr>
      </thead>
      <tbody>
        {parameters.map((parameter) => (
          <tr key={`${parameter.in}:${parameter.name}`}>
            <td>
              <code>{parameter.name}</code>
            </td>
            <td>{parameter.in}</td>
            <td>{parameter.required === true ? "yes" : "no"}</td>
            <td>{schemaLabel(parameter.schema)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function OperationBlock({
  method,
  path,
  operation,
}: {
  method: string;
  path: string;
  operation: Operation;
}): ReactNode {
  const body = requestSchema(operation);
  const responses = successResponses(operation);
  return (
    <div>
      <h3>
        <code>
          {method.toUpperCase()} {path}
        </code>
      </h3>
      {operation.summary !== undefined ? <p>{operation.summary}</p> : null}
      {operation.parameters !== undefined && operation.parameters.length > 0 ? (
        <Parameters parameters={operation.parameters} />
      ) : null}
      {body !== undefined ? (
        <p>
          Request body: <SchemaRef schema={body} />
        </p>
      ) : null}
      {responses.length > 0 ? (
        <table>
          <thead>
            <tr>
              <th>Status</th>
              <th>Body</th>
            </tr>
          </thead>
          <tbody>
            {responses.map((response) => (
              <tr key={response.status}>
                <td>
                  <code>{response.status}</code>
                </td>
                <td>
                  <SchemaRef schema={response.schema} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
    </div>
  );
}

function SchemaBlock({ name, schema }: { name: string; schema: JsonSchema }): ReactNode {
  const properties = Object.entries(schema.properties ?? {});
  const required = new Set(schema.required ?? []);
  return (
    <div>
      <h3 id={`schema-${name}`}>
        <code>{name}</code>
      </h3>
      {properties.length > 0 ? (
        <table>
          <thead>
            <tr>
              <th>Field</th>
              <th>Type</th>
              <th>Required</th>
            </tr>
          </thead>
          <tbody>
            {properties.map(([field, fieldSchema]) => (
              <tr key={field}>
                <td>
                  <code>{field}</code>
                </td>
                <td>{schemaLabel(fieldSchema)}</td>
                <td>{required.has(field) ? "yes" : "no"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p>
          <SchemaRef schema={schema} />
        </p>
      )}
    </div>
  );
}

export default function ControlPlaneApiReference(): ReactNode {
  const groups = operationsByTag();
  const schemas = Object.entries(openapi.components.schemas);
  return (
    <article>
      <h1>Control plane API</h1>
      <p>
        Generated from the control plane's OpenAPI {openapi.openapi} contract (version{" "}
        {openapi.info.version}) — the same document served at <code>GET /openapi.json</code> and
        used to generate the dashboard's client types. This page is rebuilt from that contract, so
        it cannot drift from the live surface. For the engine surface and a narrative overview, see{" "}
        <a href="/api">API reference</a>.
      </p>

      {groups.map((group) => (
        <section key={group.tag}>
          <h2>{group.tag}</h2>
          {group.operations.map((entry) => (
            <OperationBlock
              key={`${entry.method}:${entry.path}`}
              method={entry.method}
              path={entry.path}
              operation={entry.operation}
            />
          ))}
        </section>
      ))}

      <section>
        <h2>Schemas</h2>
        {schemas.map(([name, schema]) => (
          <SchemaBlock key={name} name={name} schema={schema} />
        ))}
      </section>
    </article>
  );
}
