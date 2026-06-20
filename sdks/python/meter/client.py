"""The meter engine HTTP client (standard-library only)."""

from __future__ import annotations

import json
import urllib.error
import urllib.request
from typing import Any, Callable, Optional
from urllib.parse import urlencode

from .errors import MeterError

# A transport runs one request: (method, url, body_bytes, headers) -> (status, body_bytes).
# Defaults to urllib; tests inject a fake to avoid the network.
Transport = Callable[[str, str, Optional[bytes], "dict[str, str]"], "tuple[int, bytes]"]


def _urllib_transport(
    method: str, url: str, body: Optional[bytes], headers: "dict[str, str]"
) -> "tuple[int, bytes]":
    request = urllib.request.Request(url, data=body, method=method, headers=headers)
    try:
        with urllib.request.urlopen(request) as response:  # noqa: S310 - engine URL is caller-controlled
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


class MeterClient:
    """A thin, drop-in client for the meter engine's HTTP API."""

    def __init__(self, base_url: str, transport: Optional[Transport] = None) -> None:
        self._base_url = base_url.rstrip("/")
        self._transport = transport or _urllib_transport

    def open_account(
        self,
        *,
        org_id: str,
        scope: str,
        no_overdraft: bool = False,
        parent_id: Optional[str] = None,
    ) -> dict[str, Any]:
        return self._post(
            "/v1/accounts",
            {"org_id": org_id, "scope": scope, "no_overdraft": no_overdraft, "parent_id": parent_id},
        )

    def balance(self, account: str) -> dict[str, Any]:
        return self._get(f"/v1/accounts/{account}/balance")

    def grant(
        self, account: str, *, amount: str, source: str, idempotency_key: Optional[str] = None
    ) -> dict[str, Any]:
        return self._post(
            f"/v1/accounts/{account}/grants",
            {"amount": amount, "source": source, "idempotency_key": idempotency_key},
        )

    def reserve(self, *, account: str, reservation_id: str, amount: str, limit: str) -> dict[str, Any]:
        return self._post(
            "/v1/reservations",
            {"account": account, "reservation_id": reservation_id, "amount": amount, "limit": limit},
        )

    def settle(self, reservation_id: str, actual: str) -> dict[str, Any]:
        return self._post(f"/v1/reservations/{reservation_id}/settle", {"actual": actual})

    def void_reservation(self, reservation_id: str) -> None:
        self._send("POST", f"/v1/reservations/{reservation_id}/void", None)

    def record_event(
        self,
        *,
        org_id: str,
        idempotency_key: str,
        meter: str,
        account: str,
        run_id: Optional[str] = None,
        properties: Optional[dict[str, Any]] = None,
        event_time: Optional[str] = None,
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

    def _get(self, path: str) -> Any:
        return self._send("GET", path, None)

    def _post(self, path: str, body: Optional[dict[str, Any]]) -> Any:
        return self._send("POST", path, body)

    def _send(self, method: str, path: str, body: Optional[dict[str, Any]]) -> Any:
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
