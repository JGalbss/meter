//! Tenant isolation (ADR 0007).
//!
//! An API key is either **platform**-scoped (may act across every organization — the dashboard's key)
//! or **org**-scoped (confined to its own organization). The verified [`Principal`] is the authenticated
//! caller; the auth middleware publishes it as [`CurrentPrincipal`] so handlers authorize the target org
//! against the caller instead of trusting a client-supplied org id.

import { Context, Layer } from "effect";

import type { Role } from "./rbac";

export type Scope = "platform" | "org";

const SCOPES: ReadonlySet<string> = new Set<Scope>(["platform", "org"]);

/** Whether `value` is a known scope. */
export function isScope(value: string): value is Scope {
  return SCOPES.has(value);
}

/** Coerce a stored scope string to a `Scope`, defaulting unknown values to the least privilege. */
export function toScope(value: string): Scope {
  if (isScope(value)) {
    return value;
  }
  return "org";
}

/** The authenticated caller behind a verified API key. */
export interface Principal {
  readonly orgId: string;
  readonly role: Role;
  readonly scope: Scope;
}

/**
 * The current request's principal, or `null` when auth is disabled (dev/test). Published per request by
 * the auth middleware; a default of `null` is provided where the router is served so handlers can always
 * read it.
 */
export class CurrentPrincipal extends Context.Tag("CurrentPrincipal")<
  CurrentPrincipal,
  Principal | null
>() {}

/** Default `CurrentPrincipal` (no caller). The auth middleware overrides this per authenticated request. */
export const CurrentPrincipalDefault: Layer.Layer<CurrentPrincipal> = Layer.succeed(
  CurrentPrincipal,
  null,
);

function isPlatform(principal: Principal): boolean {
  return principal.scope === "platform";
}

function ownsOrg(principal: Principal, orgId: string): boolean {
  return principal.orgId === orgId;
}

/** Whether the caller may use platform-only routes (organization CRUD). Dev no-auth is permitted. */
export function canManagePlatform(principal: Principal | null): boolean {
  if (principal === null) {
    return true;
  }
  return isPlatform(principal);
}

/** Whether the caller may mint a key of `scope`. Only platform callers may mint platform keys (an
 * org-scoped admin minting a platform key would escalate privilege). */
export function canMintScope(principal: Principal | null, scope: Scope): boolean {
  if (scope === "org") {
    return true;
  }
  return canManagePlatform(principal);
}

/** The result of authorizing a request's target org against the caller. */
export type OrgAccess =
  | { readonly allowed: true; readonly orgId: string }
  | { readonly allowed: false };

/** Whether org access was granted (narrows to the resolved org id). */
export function isAllowed(
  access: OrgAccess,
): access is { readonly allowed: true; readonly orgId: string } {
  return access.allowed;
}

/**
 * Authorize a request that targets `requestedOrgId`. Platform keys (and dev no-auth) may target any org;
 * org keys may only target their own. Returns the org id the handler should use — the caller's own org
 * for org keys, the requested org for platform keys.
 */
export function authorizeOrg(principal: Principal | null, requestedOrgId: string): OrgAccess {
  if (principal === null) {
    return { allowed: true, orgId: requestedOrgId };
  }
  if (isPlatform(principal)) {
    return { allowed: true, orgId: requestedOrgId };
  }
  if (ownsOrg(principal, requestedOrgId)) {
    return { allowed: true, orgId: principal.orgId };
  }
  return { allowed: false };
}

/**
 * The org filter for a by-id mutation whose target org is not in the request. Org keys are confined to
 * their own org (the mutation must match it); platform keys (and dev no-auth) are unconfined (`null`).
 */
export function orgScope(principal: Principal | null): string | null {
  if (principal === null) {
    return null;
  }
  if (isPlatform(principal)) {
    return null;
  }
  return principal.orgId;
}
