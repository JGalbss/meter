// Minimal typed view over the control plane's emitted OpenAPI document, plus the helpers the generated
// reference page uses to label schemas. The document is our own committed build artifact (synced by
// `scripts/sync-openapi.mjs` and drift-checked in CI), not untrusted input — so a single typed view of
// it here is safe.
import document from "./control-plane-openapi.json";

export interface JsonSchema {
  readonly type?: string;
  readonly $ref?: string;
  readonly items?: JsonSchema;
  readonly enum?: readonly string[];
  readonly properties?: Record<string, JsonSchema>;
  readonly required?: readonly string[];
  readonly anyOf?: readonly JsonSchema[];
}

export interface Parameter {
  readonly name: string;
  readonly in: string;
  readonly required?: boolean;
  readonly schema?: JsonSchema;
}

interface MediaType {
  readonly schema?: JsonSchema;
}

export interface Operation {
  readonly tags?: readonly string[];
  readonly summary?: string;
  readonly operationId?: string;
  readonly parameters?: readonly Parameter[];
  readonly requestBody?: {
    readonly required?: boolean;
    readonly content?: Record<string, MediaType>;
  };
  readonly responses?: Record<
    string,
    { readonly description?: string; readonly content?: Record<string, MediaType> }
  >;
}

export interface OpenApiDocument {
  readonly openapi: string;
  readonly info: {
    readonly title: string;
    readonly version: string;
    readonly description?: string;
  };
  readonly paths: Record<string, Record<string, Operation>>;
  readonly components: { readonly schemas: Record<string, JsonSchema> };
}

export const openapi: OpenApiDocument = document as unknown as OpenApiDocument;

/** The HTTP methods we render, in a stable display order. */
export const METHODS: readonly string[] = ["get", "post", "put", "patch", "delete"];

/** The trailing name of a `#/components/schemas/Foo` reference. */
export function refName(ref: string): string {
  return ref.slice(ref.lastIndexOf("/") + 1);
}

function isRef(schema: JsonSchema): schema is JsonSchema & { $ref: string } {
  return typeof schema.$ref === "string";
}

function isArray(schema: JsonSchema): boolean {
  return schema.type === "array" && schema.items !== undefined;
}

/** A short human label for a schema: a ref name, `Name[]`, an enum union, or a primitive type. */
export function schemaLabel(schema: JsonSchema | undefined): string {
  if (schema === undefined) {
    return "—";
  }
  if (isRef(schema)) {
    return refName(schema.$ref);
  }
  if (isArray(schema) && schema.items !== undefined) {
    return `${schemaLabel(schema.items)}[]`;
  }
  if (schema.enum !== undefined) {
    return schema.enum.map((value) => `"${value}"`).join(" | ");
  }
  if (schema.anyOf !== undefined) {
    return schema.anyOf.map(schemaLabel).join(" | ");
  }
  return schema.type ?? "object";
}

/** The component-schema name a schema references, if any — used to deep-link the label to its anchor. */
export function schemaLink(schema: JsonSchema | undefined): string | null {
  if (schema === undefined) {
    return null;
  }
  if (isRef(schema)) {
    return refName(schema.$ref);
  }
  if (isArray(schema) && schema.items !== undefined && isRef(schema.items)) {
    return refName(schema.items.$ref);
  }
  return null;
}

/** The JSON request-body schema for an operation, if it has one. */
export function requestSchema(operation: Operation): JsonSchema | undefined {
  return operation.requestBody?.content?.["application/json"]?.schema;
}

/** Successful (2xx) responses with their JSON body schema, in status order. */
export function successResponses(
  operation: Operation,
): ReadonlyArray<{ readonly status: string; readonly schema: JsonSchema | undefined }> {
  const responses = operation.responses ?? {};
  return Object.keys(responses)
    .filter((status) => status.startsWith("2"))
    .sort()
    .map((status) => ({
      status,
      schema: responses[status]?.content?.["application/json"]?.schema,
    }));
}

export interface RenderedOperation {
  readonly method: string;
  readonly path: string;
  readonly operation: Operation;
}

/** Operations grouped by their first tag, tags ordered by first appearance in the document. */
export function operationsByTag(): ReadonlyArray<{
  readonly tag: string;
  readonly operations: readonly RenderedOperation[];
}> {
  const groups = new Map<string, RenderedOperation[]>();
  for (const [path, item] of Object.entries(openapi.paths)) {
    for (const method of METHODS) {
      const operation = item[method];
      if (operation === undefined) {
        continue;
      }
      const tag = operation.tags?.[0] ?? "Other";
      const existing = groups.get(tag);
      if (existing === undefined) {
        groups.set(tag, [{ method, path, operation }]);
        continue;
      }
      existing.push({ method, path, operation });
    }
  }
  return Array.from(groups, ([tag, operations]) => ({ tag, operations }));
}
