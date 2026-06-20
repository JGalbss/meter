# @meter/control-plane

The meter **control plane**: the configuration and operational-state API the dashboard hits. It owns
config and workflow state (organizations, products, notifications, alert rules, webhooks) — **never
money**. All money-truth lives in the Rust engine; the control plane calls the engine for anything
involving credits.

Built with **Effect** (typed effects, `Schema` validation) over **Drizzle ORM** on PostgreSQL. The
HTTP surface is an `@effect/platform` `HttpRouter`, one module per resource.

## Run

```bash
# Postgres for config (separate database from the engine ledger is fine)
export METER_CONTROL_PLANE_DATABASE_URL=postgres://meter:meter@localhost:5432/meter_config
export METER_CONTROL_PLANE_PORT=8080
pnpm --filter @meter/control-plane run dev
```

Apply schema with Drizzle: `pnpm --filter @meter/control-plane run db:generate` then run the
generated SQL in `drizzle/` against the database.

## API

All bodies and responses are JSON. Validation failures return `400 {"error":"invalid", ...}`;
unknown resources return `404 {"error":"not_found", ...}`.

| Method & path | Purpose |
|---|---|
| `GET /health` | Liveness. |
| `POST /v1/organizations` | Create an organization (`slug`, `name`). |
| `GET /v1/organizations` | List organizations. |
| `POST /v1/products` | Create a product (`orgId`, `key`, `name`). |
| `GET /v1/products?orgId` | List an org's products. |
| `POST /v1/notifications` | Raise a notification (`orgId`, `type`, `severity`, `title`, `body?`, `data?`). Dispatches matching webhooks. |
| `GET /v1/notifications?orgId&status?` | Pull notifications (optionally filter by `status`). |
| `POST /v1/notifications/:id/read` | Mark read. |
| `POST /v1/notifications/:id/ack` | Acknowledge. |
| `POST /v1/alert-rules` | Create an alert rule (`orgId`, `name`, `scope`, `metric`, `threshold`, `action`, `enabled?`). |
| `GET /v1/alert-rules?orgId` | List alert rules. |
| `POST /v1/alert-rules/:id/enabled` | Enable/disable (`enabled`). |
| `POST /v1/webhooks` | Register a webhook (`orgId`, `url`, `secret`, `eventTypes?`). The secret is never returned. |
| `GET /v1/webhooks?orgId` | List webhooks (secret omitted). |
| `POST /v1/webhooks/:id/enabled` | Enable/disable (`enabled`). |
| `GET /v1/webhook-deliveries?orgId` | The delivery log (audit + dead-letter). |

### Enumerations

- notification `type`: `budget` · `credit` · `invoice` · `run_failure` · `system`
- notification `severity`: `info` · `warning` · `critical`
- alert-rule `scope`: `org` · `team` · `user` · `product`
- alert-rule `metric`: `budget` · `credit` · `spend`
- alert-rule `action`: `notify` · `webhook` · `enforce`

### Webhook signing

Every delivery carries `X-Meter-Event` and `X-Meter-Signature: sha256=<hex>` — an HMAC-SHA256 of the
raw request body keyed by the endpoint's secret. Receivers recompute and compare in constant time
(`isValidSignature` in `src/webhooks/signature.ts`). An empty `eventTypes` list subscribes to all
events. Deliveries are retried with backoff; every outcome is recorded in the delivery log.

## Test

```bash
pnpm --filter @meter/control-plane run test       # vitest, in-process HTTP server over PGlite
pnpm --filter @meter/control-plane run typecheck
```
