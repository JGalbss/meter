# meter Helm chart

Deploys the meter stack to Kubernetes: the **engine** (money-truth) and the **control plane** (config
API), with **Postgres** (money + config) and **ClickHouse** (events + audit + analytics) either
in-cluster (batteries-included) or supplied externally.

## Install

```bash
helm install meter deploy/helm/meter \
  --set engine.image.repository=ghcr.io/you/meter-engine \
  --set engine.image.tag=0.0.0 \
  --set controlPlane.image.repository=ghcr.io/you/meter-control-plane \
  --set controlPlane.image.tag=0.0.0
```

The engine applies its Postgres + ClickHouse migrations on boot; the control plane applies its config
migrations on boot. Liveness probes hit `/health` (static); readiness probes hit `/health/ready` on the
engine and control plane (which ping their stores), so traffic is gated until dependencies are reachable.

## Production: external data stores

Point at managed Postgres + ClickHouse and turn off the in-cluster ones:

```bash
helm install meter deploy/helm/meter \
  --set postgres.enabled=false \
  --set clickhouse.enabled=false \
  --set engine.databaseUrl='postgres://user:pass@pg.internal:5432/meter' \
  --set engine.clickhouseUrl='http://clickhouse.internal:8123' \
  --set engine.replicas=6
```

`engine.databaseUrl` is shared by the control plane (same Postgres); override `controlPlane.databaseUrl`
only if they must differ.

## Scaling

The engine is stateless — raise `engine.replicas`. The money store (Postgres) and a ClickHouse cluster
carry the load. A TigerBeetle ledger backend is a planned opt-in for very high ledger throughput, behind
the `LedgerBackend` trait; it is not a Helm option today. See
[`docs/adr/0005-provider-scale-throughput.md`](../../../docs/adr/0005-provider-scale-throughput.md).

## Key values

| Value | Default | Purpose |
| --- | --- | --- |
| `engine.replicas` | `2` | Stateless engine replicas. |
| `engine.image.repository` / `.tag` | ghcr placeholder / appVersion | Engine image. |
| `controlPlane.image.repository` / `.tag` | ghcr placeholder / appVersion | Control-plane image. |
| `postgres.enabled` / `clickhouse.enabled` | `true` | Run the data store in-cluster. |
| `engine.databaseUrl` / `engine.clickhouseUrl` | in-cluster | External store URLs. |
| `credentials.postgresUser` / `.postgresPassword` / `.postgresDatabase` | `meter` | In-cluster Postgres credentials (stored in a Secret). |
| `dashboard.enabled` | `true` | Deploy the operator console; `dashboard.password` / `dashboard.sessionSecret` set its login. |
| `ingress.enabled` | `false` | Expose the dashboard at `ingress.host` (set `className`, `tls.*`). |
| `ingress.controlPlaneHost` / `ingress.engineHost` | `""` | Optional hosts that route the control-plane and engine HTTP APIs (for the SDKs); empty keeps a surface cluster-internal. TLS covers every set host. |

Images for `engine` / `controlPlane` / `dashboard` are built and pushed to
`ghcr.io/<owner>/meter-*` by the repo's `release.yml` workflow on a `v*` tag.

Render the manifests without installing:

```bash
helm template meter deploy/helm/meter
```
