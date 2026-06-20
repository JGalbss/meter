#!/usr/bin/env python3
"""Cross-stack e2e: the full money flow through the real engine + Postgres + ClickHouse, then the
control plane (config, and its call back into the engine).

The engine money flow is driven through the **Python SDK**, so this doubles as the
SDK-against-a-running-engine check. Run it against a stack started by `smoke.sh`.
"""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request
import uuid

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(REPO, "sdks", "python"))

from meter import MeterClient  # noqa: E402

ENGINE = os.environ.get("ENGINE_URL", "http://localhost:8080")
CONTROL_PLANE = os.environ.get("CONTROL_PLANE_URL", "http://localhost:8090")

failures: list[str] = []


def check(name: str, ok: bool) -> None:
    print(f"  {'PASS' if ok else 'FAIL'}  {name}")
    if not ok:
        failures.append(name)


def request(method: str, url: str, body: object = None) -> tuple[int, object]:
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(
        url, data=data, method=method, headers={"content-type": "application/json"}
    )
    try:
        with urllib.request.urlopen(req) as response:
            raw = response.read()
            return response.status, (json.loads(raw) if raw else None)
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode()


print("== engine money flow (driven through the Python SDK) ==")
_, catalog = request("GET", f"{ENGINE}/v1/catalog")
has_models = isinstance(catalog, dict) and len(catalog.get("models", [])) > 0
check("catalog lists models", has_models)
model = catalog["models"][0]["model_id"] if has_models else "claude-opus-4-8"

meter = MeterClient(ENGINE)
org = str(uuid.uuid4())
account = meter.open_account(org_id=org, scope="org")["id"]
check("account opened", bool(account))

meter.grant(account, amount="100000", source="grant")
settled_after_grant = meter.balance(account)["settled"]
check("grant credited the account", settled_after_grant not in ("0", "0.0", ""))

usage = meter.meter_usage(
    org_id=org,
    account=account,
    model=model,
    idempotency_key=str(uuid.uuid4()),
    usage={"input_uncached": 10000, "output": 5000},
)
check("usage priced and charged", usage.get("charged") is True)

settled_after_usage = meter.balance(account)["settled"]
check("usage debited credits", settled_after_usage != settled_after_grant)

invoice = meter.invoice(account, "2020-01-01T00:00:00Z", "2030-01-01T00:00:00Z")
check("invoice summed from the ledger", "total_credits" in invoice)

_, budget = request(
    "GET",
    f"{ENGINE}/v1/accounts/{account}/budget"
    "?start=2020-01-01T00:00:00Z&end=2030-01-01T00:00:00Z&limit=100000",
)
budget_ok = isinstance(budget, dict) and budget.get("status") in ("ok", "warning", "exceeded")
check("engine classifies budget status", budget_ok)

_, audit = request("GET", f"{ENGINE}/v1/audit?limit=20")
check("audit log recorded the mutations", isinstance(audit, list) and len(audit) > 0)

print("== control plane (config, and its call into the engine) ==")
_, created = request(
    "POST", f"{CONTROL_PLANE}/v1/organizations", {"slug": f"e2e-{org[:8]}", "name": "E2E"}
)
cp_org = created["id"] if isinstance(created, dict) and "id" in created else None
check("organization created", cp_org is not None)

_, orgs = request("GET", f"{CONTROL_PLANE}/v1/organizations")
check(
    "organization listed",
    isinstance(orgs, list) and any(o.get("id") == cp_org for o in orgs),
)

# A budget alert rule on the engine account, then evaluate — exercises control-plane -> real engine.
_, rule = request(
    "POST",
    f"{CONTROL_PLANE}/v1/alert-rules",
    {
        "orgId": cp_org,
        "name": "e2e budget",
        "scope": "org",
        "metric": "budget",
        "threshold": 80,
        "action": "notify",
        "accountId": account,
        "creditLimit": 100000,
        "windowDays": 30,
    },
)
check("alert rule created", isinstance(rule, dict) and "id" in rule)

_, summary = request(
    "POST", f"{CONTROL_PLANE}/v1/alert-rules/evaluate?orgId={cp_org}", None
)
check(
    "control plane evaluated rules against the engine",
    isinstance(summary, dict) and summary.get("evaluated", 0) >= 1,
)

print()
if failures:
    print(f"E2E FAILED — {len(failures)} check(s) failed: {failures}")
    sys.exit(1)
print(f"E2E PASSED — all checks green (model={model})")
