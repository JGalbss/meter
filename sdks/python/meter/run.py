"""Run governance: reserve before, settle after, void on failure."""

from __future__ import annotations

import contextlib
import uuid
from collections.abc import Callable
from typing import TypeVar

from .client import MeterClient
from .errors import MeterError

T = TypeVar("T")


def with_run(
    client: MeterClient,
    *,
    account: str,
    estimate: str,
    work: Callable[[Callable[[str], None]], T],
    reservation_id: str | None = None,
    limit: str = "hard",
) -> T:
    """Run an operation under a credit reservation.

    Reserves ``estimate`` up front; if denied, the work never runs. ``work`` receives a ``settle``
    callback to post the actual usage. If ``work`` raises (or never settles), the reservation is
    voided so a failed run leaves no lingering hold.
    """
    reservation = reservation_id or str(uuid.uuid4())
    outcome = client.reserve(
        account=account, reservation_id=reservation, amount=estimate, limit=limit
    )
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


def with_run_usage(
    client: MeterClient,
    *,
    account: str,
    model: str,
    estimate: dict[str, int],
    work: Callable[[Callable[[dict[str, int]], None]], T],
    reservation_id: str | None = None,
    limit: str = "hard",
) -> T:
    """Run an operation under a token-priced reservation.

    The token ``estimate`` is priced by the engine and reserved up front; if denied, the work never
    runs. ``work`` receives a ``settle`` callback for the actual token usage; if it raises or never
    settles, the reservation is voided so a failed run leaves no lingering hold.
    """
    reservation = reservation_id or str(uuid.uuid4())
    outcome = client.reserve_usage(
        account=account,
        reservation_id=reservation,
        model=model,
        estimate=estimate,
        limit=limit,
    )
    if outcome.get("outcome") == "denied":
        raise MeterError(402, "reservation_denied", "reservation denied")

    settled = False

    def settle(actual: dict[str, int]) -> None:
        nonlocal settled
        client.settle_usage(reservation, model=model, actual=actual)
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
    # Best-effort cleanup; never mask the original error that triggered the void.
    with contextlib.suppress(Exception):
        client.void_reservation(reservation_id)
