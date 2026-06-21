# ADR 0007 — Control-plane tenant isolation (platform vs org-scoped keys)

Status: accepted. App-level enforcement is **implemented and tested**; Postgres RLS (defense-in-depth)
is the remaining layer, tracked in `/tickets` (EPIC 02 RLS). Approved as proposed — platform vs
org-scoped keys, app-level `requireOrgAccess` on reads and writes, RLS as defense-in-depth, and a
non-breaking migration (existing keys default to platform scope, new keys to org scope).

Extends DECISIONS #7 (multi-tenancy via Postgres RLS) with the concrete control-plane model.

## Implementation status

- **Done (app layer):** the api-key `scope` column (migration 0006; existing keys → `platform`, new →
  `org`), the `CurrentPrincipal` request service published by the auth middleware, `requireOrgAccess`
  on every org-scoped route (reads + writes), org-scoped by-id mutations via `byIdInOrg` (cross-org id →
  404), platform-only organization CRUD, and a no-privilege-escalation guard on minting platform keys.
  Proven by `test/tenant-isolation.test.ts`. This closes the exploitable gap.
- **Remaining (RLS defense-in-depth):** deliberately not shipped yet, because doing it safely requires:
  (1) the app to set `app.current_org` (and a platform bypass) via `SET LOCAL` inside a **per-request
  transaction**, threaded through the data-table repositories — `FORCE ROW LEVEL SECURITY` without this
  locks the app out of its own tables; (2) the app to connect as a **non-owner role** without
  `BYPASSRLS`; and (3) a **real-Postgres** integration test (e.g. testcontainers) — the control-plane
  test DB (PGlite) does **not enforce RLS** (verified), so RLS cannot be proven by the current harness
  and must not be enabled unverified. RLS is therefore its own focused change with real-PG verification,
  not an add-on to the app-layer commit.

## Context

The control plane authenticates API keys and enforces **RBAC by role** (`viewer`/`member`/`admin`,
ADR-era work), but it does **not** scope data access to the key's organization. Every `api_keys` row
carries an `org_id`, yet the auth middleware never uses it: a valid key for org A can read or write org
B's data by passing `?orgId=B` (or `orgId` in a create body). That is an exploitable multi-tenancy gap.

Two facts shape the fix:

- **The dashboard authenticates with an API key** (`METER_CONTROL_PLANE_API_KEY`, Bearer) and manages
  *all* organizations — the org switcher lists every org, and it can create orgs. So the dashboard's
  key is effectively a **platform/operator** key, not an org-scoped one.
- **Customer/programmatic keys** should be the opposite: confined to the org that owns them.

So we cannot simply "scope every key to its org" — that would break the operator console. We need an
explicit **platform vs org-scoped** distinction. DECISIONS #7 also commits us to Postgres **RLS** as
the durable mechanism; this ADR positions app-level enforcement as the primary control with RLS as
defense-in-depth.

## Decision (proposed)

### 1. Key scope

Add a **`scope`** to API keys: `platform` or `org`.

- **`platform`** keys (the operator/dashboard, internal automation): unrestricted across orgs. May call
  the cross-org endpoints (`GET/POST /v1/organizations`).
- **`org`** keys (customers): may only touch their own `org_id`'s data; the cross-org `organizations`
  endpoints are denied.

Schema options (pick one at sign-off):
- **(A) `scope text not null` column** on `api_keys` (recommended): explicit; `org_id` stays meaningful
  for `org` keys and records provenance for `platform` keys.
- **(B) nullable `org_id`**: `NULL` ⇒ platform, non-null ⇒ org-scoped. Fewer columns, but loses
  platform-key provenance and weakens the `NOT NULL` invariant.

### 2. App-level enforcement (primary control)

The auth middleware resolves the principal (`scope`, `orgId`, `role`) and exposes it to handlers. A
shared `requireOrgAccess(targetOrgId)` helper enforces: a `platform` principal passes; an `org`
principal passes only when `targetOrgId === principal.orgId`, else **403**. Every org-scoped handler
calls it with the org it touches — `targetOrgId` from `?orgId=` on reads and from the request body on
writes (so both vectors are covered, not just reads). Cross-org `organizations` endpoints require
`platform` scope.

When auth is **disabled** (`METER_REQUIRE_AUTH=false`, dev/tests) there is no principal and no scoping
— preserving the current test model.

### 3. RLS (defense-in-depth)

Per DECISIONS #7 and aligning with the engine's planned RLS: `ENABLE` + **`FORCE` ROW LEVEL SECURITY**
on org-scoped tables, policies keyed on `current_setting('app.current_org')`, set per request by a
`withTenant` wrapper from the resolved org. Layered *under* the app-level control, so a missed handler
check still can't leak across tenants. `FORCE` makes RLS apply to the table owner, so it is testable
under the PGlite suite.

## Migration & rollout

Add `scope` to `api_keys`. To avoid breaking existing deployments (whose keys — including the
dashboard's — predate this), the migration **defaults existing rows to `platform`** (no access change),
and the key-mint API gains a `scope` parameter **defaulting to `org`** for newly minted keys. Operators
then mint org-scoped customer keys going forward; the dashboard key stays `platform`. Isolation thus
rolls out without a flag day, applying to new org keys immediately and to existing keys as they are
re-minted.

## Test plan

- `org` key → another org's resource ⇒ **403**; → its own org ⇒ **200**.
- `platform` key → any org ⇒ **200**; cross-org `organizations` ⇒ **200**.
- `org` key → `organizations` list/create ⇒ **403**.
- Auth disabled ⇒ unchanged (existing 28 control-plane tests stay valid).
- If RLS lands: a conformance test that a tenant-scoped connection sees only its rows (FORCE RLS under
  PGlite).

## Consequences

- Closes the cross-org data leak for customer keys; the operator console keeps working unchanged (its
  key is `platform`).
- Cost: a schema migration, a `scope` concept in the mint API (and the dashboard's key form), and an
  `requireOrgAccess` call in every org-scoped handler. RLS adds migrations + a `withTenant` wrapper.

## Open questions for sign-off

1. Approve the **platform vs org-scoped** key model (vs another tenancy model)?
2. **App-level first** (this ADR), with RLS as a fast follow — or **RLS-first**?
3. Schema **(A) `scope` column** (recommended) or **(B) nullable `org_id`**?
4. Default scope for newly minted keys = **`org`** (recommended)?

On sign-off this ADR moves to *accepted* and implementation proceeds; the `/tickets` entry tracks it.
