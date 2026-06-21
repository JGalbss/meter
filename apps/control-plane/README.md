# @meter/control-plane

The meter **control plane**: the configuration and operational-state API the dashboard hits. It owns
config and workflow state (organizations, products, notifications, alert rules, webhooks) — **never
money**. All money-truth lives in the Rust engine; the control plane calls the engine for anything
involving credits.

Built with **Effect** (typed effects, `Schema` validation) over **Drizzle ORM** on PostgreSQL. The
HTTP surface is an `@effect/platform` `HttpRouter`, one module per resource.

## Run

```bash
# Postgres for config (a separate database from the engine ledger is fine)
export METER_CONTROL_PLANE_DATABASE_URL=postgres://meter:meter@localhost:5432/meter_config
pnpm --filter @meter/control-plane run dev   # tsx watch; serves http://localhost:8090
```

Drizzle migrations apply automatically on boot. Generate a migration after a schema change with
`pnpm --filter @meter/control-plane run db:generate`.

| Variable | Default | Purpose |
|---|---|---|
| `METER_CONTROL_PLANE_DATABASE_URL` | required | Postgres for the config schema. |
| `METER_CONTROL_PLANE_PORT` | `8090` | HTTP listen port. |
| `METER_ENGINE_URL` | `http://127.0.0.1:8080` | The engine, asked to classify budget usage during alert evaluation. |
| `METER_EVALUATION_INTERVAL_SECONDS` | `0` | Alert scheduler interval; `0` disables it (evaluate on demand only). |
| `METER_REQUIRE_AUTH` | `false` | When `true`, require a Bearer key on every route except `/health`. |

## API

All bodies and responses are JSON. Validation failures return `400 {"error":"invalid", ...}`;
unknown resources return `404 {"error":"not_found", ...}`.

| Method & path | Purpose |
|---|---|
| `GET /health` · `GET /health/ready` | Liveness; readiness pings the database. |
| `GET /openapi.json` | The OpenAPI document the dashboard client is generated from. |
| `POST /v1/organizations` | Create an organization (`slug`, `name`). |
| `GET /v1/organizations` | List organizations. |
| `POST /v1/products` | Create a product (`orgId`, `key`, `name`). |
| `GET /v1/products?orgId` | List an org's products. |
| `POST /v1/notifications` | Raise a notification (`orgId`, `type`, `severity`, `title`, `body?`, `data?`). Dispatches matching webhooks. |
| `GET /v1/notifications?orgId&status?` | Pull notifications (optionally filter by `status`). |
| `POST /v1/notifications/:id/read` | Mark read. |
| `POST /v1/notifications/:id/ack` | Acknowledge. |
| `POST /v1/alert-rules` | Create an alert rule (`orgId`, `name`, `scope`, `metric`, `threshold`, `action`, `enabled?`; budget rules also take `accountId`, `creditLimit`, `windowDays?`). |
| `GET /v1/alert-rules?orgId` | List alert rules. |
| `POST /v1/alert-rules/:id/enabled` | Enable/disable (`enabled`). |
| `POST /v1/alert-rules/evaluate?orgId` | Evaluate budget rules against the engine; raise notifications + fire webhooks on escalation. Returns `{ evaluated, raised }`. |
| `POST /v1/webhooks` | Register a webhook (`orgId`, `url`, `secret`, `eventTypes?`). The secret is never returned. |
| `GET /v1/webhooks?orgId` | List webhooks (secret omitted). |
| `POST /v1/webhooks/:id/enabled` | Enable/disable (`enabled`). |
| `GET /v1/webhook-deliveries?orgId` | The delivery log (audit + dead-letter). |
| `POST /v1/api-keys` | Mint an API key (`orgId`, `name`, `role?`, `scope?`). Returns the plaintext token **once** (`mk_…`). |
| `GET /v1/api-keys?orgId` | List API keys (never the token or its hash). |
| `POST /v1/api-keys/:id/revoke` | Revoke an API key. |

### Enumerations

- notification `type`: `budget` · `credit` · `invoice` · `run_failure` · `system`
- notification `severity`: `info` · `warning` · `critical`
- alert-rule `scope`: `org` · `team` · `user` · `product`
- alert-rule `metric`: `budget` · `credit` · `spend`
- alert-rule `action`: `notify` · `webhook` · `enforce`

### Budget alert evaluation

`POST /v1/alert-rules/evaluate` asks the **engine** (`METER_ENGINE_URL`, default
`http://127.0.0.1:8080`) to classify each budget rule's account usage over its rolling window against
its `creditLimit`. The control plane computes no money — it reacts to the engine's
`ok`/`warning`/`exceeded` status. Alerts fire on *escalation* (status transitions up), so a sustained
breach does not spam; each raised notification also dispatches matching webhooks.

Set `METER_EVALUATION_INTERVAL_SECONDS` (> 0) to run the built-in scheduler, which evaluates every
organization's budget rules on that interval. The default `0` disables it — evaluate on demand via the
endpoint instead.

### Authentication and RBAC

Set `METER_REQUIRE_AUTH=true` to require `Authorization: Bearer <token>` on every route except
`/health`. A missing or invalid key is `401`; an insufficient role is `403`.

Each key carries a **role**, ranked `viewer < member < admin`, and the requirement is method- and
path-driven: reads (`GET`) need `viewer`, ordinary writes need `member`, and credential routes
(`/v1/api-keys`, `/v1/webhooks`) need `admin`. A higher role satisfies any lower requirement; a key
minted without a role defaults to `admin`.

Each key also carries a **scope**: `platform` keys act across any org and can mint platform keys; `org`
keys are confined to their own org (the default). The token format is `mk_<base64url(24 bytes)>`, shown
once on mint — only its SHA-256 hash is stored. Bootstrap the first key with auth disabled, then enable
enforcement. The dashboard sends its key when `METER_CONTROL_PLANE_API_KEY` is set.

### Webhook signing

Every delivery carries `x-meter-event: <type>` and `x-meter-signature: sha256=<hex>` — an HMAC-SHA256
of the raw request body keyed by the endpoint's secret. Receivers recompute and compare in constant
time (`isValidSignature` in `src/webhooks/signature.ts`). The payload is
`{ event, notification: { id, orgId, type, severity, title, body, data, createdAt } }`. An empty
`eventTypes` list subscribes to all events. Deliveries are retried with backoff; every outcome is
recorded in the delivery log.

## Test

```bash
pnpm --filter @meter/control-plane run test       # vitest, in-process HTTP server over PGlite
pnpm --filter @meter/control-plane run typecheck
```
