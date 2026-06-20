"""Tests for the meter Python SDK (standard-library unittest, fake transport)."""

from __future__ import annotations

import json
import unittest
from typing import Any, Callable

from meter import (
    MeterClient,
    MeterError,
    anthropic_usage,
    bedrock_usage,
    gemini_usage,
    openai_usage,
    record_model_usage,
    with_run,
)

Handler = Callable[[str, str], "tuple[int, Any]"]


def make_transport(handler: Handler):
    calls: list[tuple[str, str, Any]] = []

    def transport(method, url, body, headers):
        decoded = None if body is None else json.loads(body.decode())
        calls.append((method, url, decoded))
        status, payload = handler(method, url)
        return status, json.dumps(payload).encode()

    return transport, calls


class ClientTests(unittest.TestCase):
    def test_open_account_maps_to_wire_shape(self) -> None:
        transport, calls = make_transport(
            lambda _m, _u: (
                200,
                {"id": "acc-1", "org_id": "org-1", "scope": "org", "no_overdraft": True, "parent_id": None},
            )
        )
        client = MeterClient("http://engine", transport)
        account = client.open_account(org_id="org-1", scope="org", no_overdraft=True)
        self.assertEqual(account["id"], "acc-1")
        method, url, body = calls[0]
        self.assertEqual(method, "POST")
        self.assertEqual(url, "http://engine/v1/accounts")
        self.assertEqual(body["org_id"], "org-1")

    def test_non_2xx_raises_meter_error(self) -> None:
        transport, _ = make_transport(
            lambda _m, _u: (404, {"error": "not_found", "message": "account not found"})
        )
        client = MeterClient("http://engine", transport)
        with self.assertRaises(MeterError) as ctx:
            client.balance("missing")
        self.assertEqual(ctx.exception.status, 404)
        self.assertEqual(ctx.exception.code, "not_found")


class AdapterTests(unittest.TestCase):
    def test_anthropic_usage(self) -> None:
        usage = anthropic_usage(
            {
                "input_tokens": 1000,
                "output_tokens": 500,
                "cache_read_input_tokens": 200,
                "cache_creation_input_tokens": 50,
            }
        )
        self.assertEqual(
            (usage.input_uncached, usage.cache_read, usage.cache_write, usage.output),
            (1000, 200, 50, 500),
        )

    def test_openai_usage_subtracts_cached(self) -> None:
        usage = openai_usage(
            {
                "prompt_tokens": 1000,
                "completion_tokens": 500,
                "prompt_tokens_details": {"cached_tokens": 200},
                "completion_tokens_details": {"reasoning_tokens": 120},
            }
        )
        self.assertEqual(
            (usage.input_uncached, usage.cache_read, usage.output, usage.reasoning),
            (800, 200, 500, 120),
        )

    def test_gemini_usage_subtracts_cached(self) -> None:
        usage = gemini_usage(
            {
                "promptTokenCount": 1000,
                "candidatesTokenCount": 400,
                "cachedContentTokenCount": 250,
                "thoughtsTokenCount": 60,
            }
        )
        self.assertEqual(
            (usage.input_uncached, usage.cache_read, usage.output, usage.reasoning),
            (750, 250, 400, 60),
        )

    def test_bedrock_usage(self) -> None:
        usage = bedrock_usage(
            {
                "inputTokens": 900,
                "outputTokens": 300,
                "cacheReadInputTokens": 100,
                "cacheWriteInputTokens": 20,
            }
        )
        self.assertEqual(
            (usage.input_uncached, usage.cache_read, usage.cache_write, usage.output),
            (900, 100, 20, 300),
        )

    def test_record_model_usage_emits_event(self) -> None:
        transport, calls = make_transport(lambda _m, _u: (200, {"id": "evt-1", "status": "recorded"}))
        client = MeterClient("http://engine", transport)
        record_model_usage(
            client,
            org_id="org-1",
            account="acc-1",
            model="claude-opus-4-8",
            idempotency_key="u1",
            run_id="run-1",
            usage=anthropic_usage({"input_tokens": 1000, "output_tokens": 500}),
        )
        _method, url, body = calls[0]
        self.assertTrue(url.endswith("/v1/events"))
        self.assertEqual(body["run_id"], "run-1")
        self.assertEqual(body["properties"]["model"], "claude-opus-4-8")
        self.assertEqual(body["properties"]["input_uncached"], 1000)


class RunTests(unittest.TestCase):
    def test_with_run_reserves_then_settles(self) -> None:
        def handler(_method: str, url: str):
            if url.endswith("/reservations"):
                return 200, {"outcome": "allowed", "reservation": "res-1"}
            if url.endswith("/settle"):
                return 200, {"id": "e1"}
            return 200, {}

        transport, calls = make_transport(handler)
        client = MeterClient("http://engine", transport)

        def work(settle):
            settle("30")
            return "done"

        result = with_run(client, account="acc-1", estimate="40", reservation_id="res-1", work=work)
        self.assertEqual(result, "done")
        urls = [url for _m, url, _b in calls]
        self.assertTrue(any(u.endswith("/settle") for u in urls))
        self.assertFalse(any(u.endswith("/void") for u in urls))

    def test_with_run_voids_on_error(self) -> None:
        def handler(_method: str, url: str):
            if url.endswith("/reservations"):
                return 200, {"outcome": "allowed", "reservation": "res-1"}
            return 200, {}

        transport, calls = make_transport(handler)
        client = MeterClient("http://engine", transport)

        def work(_settle):
            raise RuntimeError("boom")

        with self.assertRaises(RuntimeError):
            with_run(client, account="acc-1", estimate="40", reservation_id="res-1", work=work)
        urls = [url for _m, url, _b in calls]
        self.assertTrue(any(u.endswith("/void") for u in urls))

    def test_with_run_raises_when_denied(self) -> None:
        transport, _ = make_transport(
            lambda _m, _u: (200, {"outcome": "denied", "available": "5", "requested": "40"})
        )
        client = MeterClient("http://engine", transport)
        with self.assertRaises(MeterError):
            with_run(client, account="acc-1", estimate="40", reservation_id="res-1", work=lambda _s: "x")


if __name__ == "__main__":
    unittest.main()
