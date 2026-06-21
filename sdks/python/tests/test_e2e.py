"""End-to-end SDK test against a **real running engine** (real urllib transport, not a fake).

Opt-in: runs only when ``METER_E2E_BASE_URL`` points at a live engine, so the normal test run stays
fast and offline. Bring up the stack and run it with the TypeScript SDK's ``test/e2e/run.sh`` (it
starts the same engine), then:

    METER_E2E_BASE_URL=http://127.0.0.1:8080 python3 -m unittest tests.test_e2e

It drives the full money path over the wire — the contract the SDK and engine must agree on.
"""

from __future__ import annotations

import os
import unittest
import uuid
from datetime import datetime, timezone

from meter import MeterClient

BASE_URL = os.environ.get("METER_E2E_BASE_URL")


@unittest.skipUnless(BASE_URL, "set METER_E2E_BASE_URL to run against a live engine")
class E2ETests(unittest.TestCase):
    def setUp(self) -> None:
        self.client = MeterClient(BASE_URL or "")

    def test_open_grant_reserve_settle_balance_deny(self) -> None:
        org_id = str(uuid.uuid4())
        account = self.client.open_account(org_id=org_id, scope="org", no_overdraft=True)
        self.assertTrue(account["id"])
        self.assertTrue(account["no_overdraft"])

        self.client.grant(account["id"], amount="1000", source="paid")
        self.assertEqual(float(self.client.balance(account["id"])["settled"]), 1000)

        reservation_id = str(uuid.uuid4())
        reserved = self.client.reserve(
            account=account["id"], reservation_id=reservation_id, amount="100", limit="hard"
        )
        self.assertEqual(reserved["outcome"], "allowed")
        self.assertEqual(float(self.client.balance(account["id"])["held"]), 100)

        self.client.settle(reservation_id, "60")
        balance = self.client.balance(account["id"])
        self.assertEqual(float(balance["settled"]), 940)  # 1000 - 60 actual
        self.assertEqual(float(balance["held"]), 0)

        denied = self.client.reserve(
            account=account["id"],
            reservation_id=str(uuid.uuid4()),
            amount="100000",
            limit="hard",
        )
        self.assertEqual(denied["outcome"], "denied")

        now = datetime.now(timezone.utc)
        start = now.replace(day=1, hour=0, minute=0, second=0, microsecond=0).isoformat()
        end = now.replace(year=now.year + 1).isoformat()
        invoice = self.client.invoice(account["id"], start, end)
        self.assertEqual(float(invoice["total_credits"]), 60)

    def test_record_event_is_idempotent(self) -> None:
        org_id = str(uuid.uuid4())
        account = self.client.open_account(org_id=org_id, scope="org")
        key = str(uuid.uuid4())
        props = {"model": "claude-opus-4-8", "output": 500}

        event = self.client.record_event(
            org_id=org_id,
            idempotency_key=key,
            meter="tokens",
            account=account["id"],
            properties=props,
        )
        self.assertEqual(event["status"], "recorded")

        replay = self.client.record_event(
            org_id=org_id,
            idempotency_key=key,
            meter="tokens",
            account=account["id"],
            properties=props,
        )
        self.assertEqual(replay["id"], event["id"])


if __name__ == "__main__":
    unittest.main()
