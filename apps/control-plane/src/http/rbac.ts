//! Role-based access control for the control-plane API.
//!
//! Every API key carries a role. Enforcement is coarse and method/path-driven: reads need `viewer`,
//! writes need `member`, and managing credentials (API keys, webhooks) needs `admin`. Roles are
//! ranked, so a higher role satisfies any lower requirement.

export type Role = "viewer" | "member" | "admin";

const RANK: Record<Role, number> = { viewer: 0, member: 1, admin: 2 };

/** Methods that only read state. */
const READ_METHODS: ReadonlySet<string> = new Set(["GET", "HEAD", "OPTIONS"]);

/** Path prefixes whose mutations are restricted to admins (credential management). */
const ADMIN_PREFIXES: readonly string[] = ["/v1/api-keys", "/v1/webhooks"];

/** Whether `value` is a known role. */
export function isRole(value: string): value is Role {
  return value in RANK;
}

/** Coerce a stored role string to a `Role`, defaulting unknown/legacy values to `admin`. */
export function toRole(value: string): Role {
  if (isRole(value)) {
    return value;
  }
  return "admin";
}

function isReadOnly(method: string): boolean {
  return READ_METHODS.has(method.toUpperCase());
}

function isAdminScoped(path: string): boolean {
  return ADMIN_PREFIXES.some((prefix) => path.startsWith(prefix));
}

/** The minimum role required to perform `method` on `path`. */
export function requiredRole(method: string, path: string): Role {
  if (isReadOnly(method)) {
    return "viewer";
  }
  if (isAdminScoped(path)) {
    return "admin";
  }
  return "member";
}

/** Whether a principal holding `actual` may act where `required` is needed. */
export function roleSatisfies(actual: Role, required: Role): boolean {
  return RANK[actual] >= RANK[required];
}
