// Renders a generated API reference from a committed OpenAPI document: every operation grouped by tag,
// then every component schema. Shared by both surfaces (engine, control plane) — each page supplies its
// own document and intro. The documents are synced from the emitted specs and drift-checked in CI, so a
// rendered reference can never fall behind its contract.
import type { ReactNode } from "react";

import {
  type JsonSchema,
  type OpenApiDocument,
  type Operation,
  type Parameter,
  operationsByTag,
  requestSchema,
  schemaLabel,
  schemaLink,
  successResponses,
} from "../lib/openapi";

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

export function ApiReference({
  title,
  intro,
  document,
}: {
  title: string;
  intro: ReactNode;
  document: OpenApiDocument;
}): ReactNode {
  const groups = operationsByTag(document);
  const schemas = Object.entries(document.components.schemas);
  return (
    <article>
      <h1>{title}</h1>
      {intro}

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
