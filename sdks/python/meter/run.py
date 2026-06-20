"""Run governance: reserve before, settle after, void on failure."""

from __future__ import annotations

import uuid
from typing import Any, Callable, Optional, TypeVar

from .client import MeterClient
from .errors import MeterError

T = TypeVar("T")


def with_run(
    client: MeterClient,
    *,
    account: str,
    estimate: str,
    work: Callable[[Callable[[str], None]], T],
    reservation_id: Optional[str] = None,
    limit: str = "hard",
) -> T:
    """Run an operation under a credit reservation.

    Reserves ``estimate`` up front; if denied, the work never runs. ``work`` receives a ``settle``
    callback to post the actual usage. If ``work`` raises (or never settles), the reservation is
    voided so a failed run leaves no lingering hold.
    """
    reservation = reservation_id or str(uuid.uuid4())
    outcome = client.reserve(account=account, reservation_id=reservation, amount=estimate, limit=limit)
    if outcome.get("outcome") == "denied":
        raise MeterError(402, "reservation_denied", "reservation denied")

    settled = False

    def settle(actual: str) -> None:
        nonlocal settled
        client.settle(reservation, actual)
        settled = True

    try:
        result = work(settle)
        if not settled:
            client.void_reservation(reservation)
        return result
    except Exception:
        if not settled:
            _safe_void(client, reservation)
        raise


def _safe_void(client: MeterClient, reservation_id: str) -> None:
    try:
        client.void_reservation(reservation_id)
    except Exception:  # noqa: BLE001 - best-effort cleanup; surface the original error
        pass
