# meter — dashboard

The operator console for meter: a Next.js app for inspecting the ledger, running usage analytics, and
managing control-plane config. It reads authoritative data from the engine and writes config to the
control plane — it computes no money and stores no state of its own.

Built on Next.js 16 (App Router), the shadcn preset `b1z2hUjZ5c` (Dropbox aesthetic), Phosphor icons,
Recharts, and transitions.dev animations. Server Components and Server Actions only; there is no
client-side data layer.

## Pages

| Route | What it does |
|---|---|
| `/` | Overview: org-level totals, recent usage, and health of the engine and control plane. |
| `/organizations` | Create and list organizations (control plane). |
| `/products` | Per-org product catalog (control plane). |
| `/api-keys` | Mint, list, and revoke API keys with a role (`viewer`/`member`/`admin`) and scope (`platform`/`org`). The token is shown once. |
| `/notifications` | The notification feed; mark read or acknowledge. |
| `/alerts` | Budget alert rules: create, enable/disable, and evaluate against the engine on demand. |
| `/webhooks` | Register webhook endpoints and inspect the delivery log (including dead-letters). |
| `/usage` | Usage analytics: by model and by day, charted straight from the engine. |
| `/events` | Events explorer with amend (record a corrected version) and void-run (reverse a run's ledger effects). |
| `/accounts` | Accounts, balances (settled/held), and ledger entries. |
| `/invoices` | Per-account invoices summed straight from the ledger. |
| `/audit` | The engine audit log, filterable by actor, method, and time. |
| `/rate-cards` | The hosted model rate-card catalog (read-only). |
| `/simulate` | Pricing simulator: re-rate a usage stream across two models. |

Reads degrade gracefully — if the engine or control plane is down, a page renders an empty state
rather than crashing.

## Run

The dashboard is two hops: config goes to the control plane, read views come from the engine. Bring
both up first (see the repo `README` and `deploy/`), then:

```bash
bun install
bun run dev      # http://localhost:3000
```

Auth is a signed-cookie session (HMAC-SHA256, 12-hour expiry). Login checks a shared admin password.
With neither secret set, the dashboard is locked.

```bash
export DASHBOARD_PASSWORD=changeme            # the login password; empty = locked
export DASHBOARD_SESSION_SECRET=$(openssl rand -hex 32)  # signs the session cookie; empty = login fails
```

### Environment

| Variable | Default | Purpose |
|---|---|---|
| `METER_CONTROL_PLANE_URL` | `http://127.0.0.1:8090` | Config reads and writes. |
| `METER_ENGINE_URL` | `http://127.0.0.1:8080` | Authoritative read views (ledger, usage, audit). |
| `DASHBOARD_PASSWORD` | empty (locked) | Shared admin login password. |
| `DASHBOARD_SESSION_SECRET` | empty (login fails) | Key that signs the session cookie. |
| `METER_CONTROL_PLANE_API_KEY` | unset | Optional Bearer key sent to the control plane. |
| `METER_ENGINE_API_KEY` | unset | Optional Bearer key sent to the engine. |
| `NEXT_PUBLIC_DOCS_URL` | GitHub `docs/` tree | Where the in-app docs link points. |

## Build and check

```bash
bun run build       # production build
bun run typecheck   # tsc --noEmit
bun run test        # vitest
bun run gen:api     # regenerate the control-plane client types from ../control-plane/openapi.json
```

UI work is gated by react-doctor (`bun run doctor`) and must use the design system — see the
`meter-design-system` skill. Do not hand-roll primitives or ad-hoc transitions.
