"""The meter engine HTTP client (standard-library only)."""

from __future__ import annotations

import json
import urllib.error
import urllib.request
from collections.abc import Callable
from typing import Any
from urllib.parse import urlencode

from .errors import MeterError

# A transport runs one request: (method, url, body_bytes, headers) -> (status, body_bytes).
# Defaults to urllib; tests inject a fake to avoid the network.
Transport = Callable[[str, str, bytes | None, "dict[str, str]"], "tuple[int, bytes]"]


def _urllib_transport(
    method: str, url: str, body: bytes | None, headers: dict[str, str]
) -> tuple[int, bytes]:
    request = urllib.request.Request(url, data=body, method=method, headers=headers)
    try:
        with urllib.request.urlopen(request) as response:  # noqa: S310 - engine URL is caller-controlled
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


class MeterClient:
    """A thin, drop-in client for the meter engine's HTTP API."""

    def __init__(self, base_url: str, transport: Transport | None = None) -> None:
        self._base_url = base_url.rstrip("/")
        self._transport = transport or _urllib_transport

    def open_account(
        self,
        *,
        org_id: str,
        scope: str,
        no_overdraft: bool = False,
        parent_id: str | None = None,
    ) -> dict[str, Any]:
        return self._post(
            "/v1/accounts",
            {
                "org_id": org_id,
                "scope": scope,
                "no_overdraft": no_overdraft,
                "parent_id": parent_id,
            },
        )

    def balance(self, account: str) -> dict[str, Any]:
        return self._get(f"/v1/accounts/{account}/balance")

    def grant(
        self, account: str, *, amount: str, source: str, idempotency_key: str | None = None
    ) -> dict[str, Any]:
        return self._post(
            f"/v1/accounts/{account}/grants",
            {"amount": amount, "source": source, "idempotency_key": idempotency_key},
        )

    def entries(self, account: str) -> list[dict[str, Any]]:
        """List the account's ledger entries (the immutable double-entry postings)."""
        return self._get(f"/v1/accounts/{account}/entries")

    def reserve(
        self,
        *,
        account: str,
        reservation_id: str,
        amount: str,
        limit: str,
        expires_at: str | None = None,
    ) -> dict[str, Any]:
        return self._post(
            "/v1/reservations",
            {
                "account": account,
                "reservation_id": reservation_id,
                "amount": amount,
                "limit": limit,
                "expires_at": expires_at,
            },
        )

    def settle(self, reservation_id: str, actual: str) -> dict[str, Any]:
        return self._post(f"/v1/reservations/{reservation_id}/settle", {"actual": actual})

    def extend_reservation(self, reservation_id: str, expires_at: str) -> None:
        """Push out a hold's expiry (RFC3339) — a heartbeat so a long run's hold isn't swept."""
        self._send(
            "POST",
            f"/v1/reservations/{reservation_id}/extend",
            {"expires_at": expires_at},
        )

    def void_reservation(self, reservation_id: str) -> None:
        self._send("POST", f"/v1/reservations/{reservation_id}/void", None)

    def open_lease(self, *, parent: str, amount: str) -> dict[str, Any]:
        """Open a per-session lease: a child account funded by a transfer from a parent."""
        return self._post("/v1/leases", {"parent": parent, "amount": amount})

    def close_lease(self, lease_id: str) -> str:
        """Close a lease, returning its unused balance to the parent."""
        body = self._post(f"/v1/leases/{lease_id}/close", None)
        return str(body["returned"])

    def record_event(
        self,
        *,
        org_id: str,
        idempotency_key: str,
        meter: str,
        account: str,
        run_id: str | None = None,
        properties: dict[str, Any] | None = None,
        event_time: str | None = None,
    ) -> dict[str, Any]:
        return self._post(
            "/v1/events",
            {
                "org_id": org_id,
                "idempotency_key": idempotency_key,
                "meter": meter,
                "account": account,
                "run_id": run_id,
                "properties": properties or {},
                "event_time": event_time,
            },
        )

    def amend_event(self, event_id: str, properties: dict[str, Any]) -> dict[str, Any]:
        return self._post(f"/v1/events/{event_id}/amend", {"properties": properties})

    def list_events(self, account: str) -> list[dict[str, Any]]:
        return self._get(f"/v1/accounts/{account}/events")

    def void_run(self, run_id: str) -> int:
        body = self._post(f"/v1/runs/{run_id}/void", None)
        return int(body["voided"])

    def invoice(self, account: str, start: str, end: str) -> dict[str, Any]:
        query = urlencode({"start": start, "end": end})
        return self._get(f"/v1/accounts/{account}/invoice?{query}")

    def meter_usage(
        self,
        *,
        org_id: str,
        account: str,
        model: str,
        idempotency_key: str,
        usage: dict[str, int],
        run_id: str | None = None,
    ) -> dict[str, Any]:
        """Price token usage via the catalog, record the event, and charge credits (idempotent)."""
        return self._post(
            "/v1/usage",
            {
                "org_id": org_id,
                "account": account,
                "model": model,
                "idempotency_key": idempotency_key,
                "run_id": run_id,
                "usage": usage,
            },
        )

    def reserve_usage(
        self,
        *,
        account: str,
        reservation_id: str,
        model: str,
        estimate: dict[str, int],
        limit: str,
        rate_card_id: str | None = None,
    ) -> dict[str, Any]:
        """Reserve a hold sized to a worst-case token estimate priced against a model (the engine
        prices it). Settle the actuals with ``settle_usage``."""
        return self._post(
            "/v1/usage/reserve",
            {
                "account": account,
                "reservation_id": reservation_id,
                "model": model,
                "estimate": estimate,
                "limit": limit,
                "rate_card_id": rate_card_id,
            },
        )

    def settle_usage(
        self,
        reservation_id: str,
        *,
        model: str,
        actual: dict[str, int],
        rate_card_id: str | None = None,
    ) -> dict[str, Any]:
        """Settle a usage-priced reservation against actual usage (the engine reprices)."""
        return self._post(
            f"/v1/usage/reservations/{reservation_id}/settle",
            {"model": model, "actual": actual, "rate_card_id": rate_card_id},
        )

    def catalog(self) -> dict[str, Any]:
        """The hosted model rate-card catalog — provider-cost prices per token."""
        return self._get("/v1/catalog")

    def simulate(
        self,
        *,
        current_model: str,
        proposed_model: str,
        events: list[dict[str, int]],
    ) -> dict[str, Any]:
        """Re-rate a usage stream from one catalogued model onto another to compare credit cost."""
        return self._post(
            "/v1/simulate",
            {
                "current_model": current_model,
                "proposed_model": proposed_model,
                "events": events,
            },
        )

    def _get(self, path: str) -> Any:
        return self._send("GET", path, None)

    def _post(self, path: str, body: dict[str, Any] | None) -> Any:
        return self._send("POST", path, body)

    def _send(self, method: str, path: str, body: dict[str, Any] | None) -> Any:
        data = None if body is None else json.dumps(body).encode("utf-8")
        status, raw = self._transport(
            method, f"{self._base_url}{path}", data, {"content-type": "application/json"}
        )
        text = raw.decode("utf-8") if raw else ""
        if status < 200 or status >= 300:
            raise _to_error(status, text)
        if not text:
            return None
        return json.loads(text)


def _to_error(status: int, text: str) -> MeterError:
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        return MeterError(status, "error", text)
    return MeterError(status, str(parsed.get("error", "error")), str(parsed.get("message", text)))
