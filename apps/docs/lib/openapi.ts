// Minimal typed view over an emitted OpenAPI document, plus the helpers the generated reference pages
// use to label schemas. Both surfaces are rendered through here: the control plane (OpenAPI 3.1, `anyOf`
// nullables) and the engine (OpenAPI 3.0.3, `oneOf`/`allOf`/`nullable`). The documents are our own
// committed build artifacts (synced by `scripts/sync-openapi.mjs` and drift-checked in CI), not
// untrusted input — so a single typed view of them here is safe. The per-surface documents are bound in
// `lib/control-plane-spec.ts` and `lib/engine-spec.ts`.

export interface JsonSchema {
  readonly type?: string;
  readonly format?: string;
  readonly $ref?: string;
  readonly items?: JsonSchema;
  readonly enum?: readonly string[];
  readonly properties?: Record<string, JsonSchema>;
  readonly required?: readonly string[];
  readonly anyOf?: readonly JsonSchema[];
  readonly oneOf?: readonly JsonSchema[];
  readonly allOf?: readonly JsonSchema[];
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

/** A primitive label, qualified by `format` when present (`integer (int64)`, `string (date-time)`). */
function primitiveLabel(schema: JsonSchema): string {
  const base = schema.type ?? "object";
  if (schema.format === undefined) {
    return base;
  }
  return `${base} (${schema.format})`;
}

/** The first member that is a `$ref`, by name — covers utoipa's `allOf: [{$ref}]` nullable wrapper. */
function firstRefName(schemas: readonly JsonSchema[]): string | null {
  for (const member of schemas) {
    if (isRef(member)) {
      return refName(member.$ref);
    }
  }
  return null;
}

/**
 * A short human label for a schema: a ref name, `Name[]`, an enum/union (`enum`, `oneOf`, `anyOf`), an
 * `allOf` intersection, or a primitive type (qualified by `format`).
 */
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
  if (schema.oneOf !== undefined) {
    return schema.oneOf.map(schemaLabel).join(" | ");
  }
  if (schema.anyOf !== undefined) {
    return schema.anyOf.map(schemaLabel).join(" | ");
  }
  if (schema.allOf !== undefined) {
    return schema.allOf.map(schemaLabel).join(" & ");
  }
  return primitiveLabel(schema);
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
  if (schema.allOf !== undefined) {
    return firstRefName(schema.allOf);
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
export function operationsByTag(document: OpenApiDocument): ReadonlyArray<{
  readonly tag: string;
  readonly operations: readonly RenderedOperation[];
}> {
  const groups = new Map<string, RenderedOperation[]>();
  for (const [path, item] of Object.entries(document.paths)) {
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
