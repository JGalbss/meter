"""Tests for the meter Python SDK (standard-library unittest, fake transport)."""

from __future__ import annotations

import json
import unittest
from collections.abc import Callable
from typing import Any

from meter import (
    MeterClient,
    MeterError,
    anthropic_usage,
    bedrock_usage,
    gemini_usage,
    langchain_usage,
    meter_model_usage,
    metered_call,
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
                {
                    "id": "acc-1",
                    "org_id": "org-1",
                    "scope": "org",
                    "no_overdraft": True,
                    "parent_id": None,
                },
            )
        )
        client = MeterClient("http://engine", transport)
        account = client.open_account(org_id="org-1", scope="org", no_overdraft=True)
        self.assertEqual(account["id"], "acc-1")
        method, url, body = calls[0]
        self.assertEqual(method, "POST")
        self.assertEqual(url, "http://engine/v1/accounts")
        self.assertEqual(body["org_id"], "org-1")

    def test_open_and_close_lease(self) -> None:
        def handler(_method: str, url: str):
            if url.endswith("/leases"):
                return 200, {"id": "lease-1", "scope": "session", "parent_id": "acc-1"}
            return 200, {"returned": "40"}

        transport, calls = make_transport(handler)
        client = MeterClient("http://engine", transport)

        lease = client.open_lease(parent="acc-1", amount="60")
        self.assertEqual(lease["id"], "lease-1")
        self.assertEqual(lease["scope"], "session")
        _method, url, body = calls[0]
        self.assertTrue(url.endswith("/v1/leases"))
        self.assertEqual(body, {"parent": "acc-1", "amount": "60"})

        returned = client.close_lease("lease-1")
        self.assertEqual(returned, "40")
        self.assertTrue(any(u.endswith("/v1/leases/lease-1/close") for _m, u, _b in calls))

    def test_non_2xx_raises_meter_error(self) -> None:
        transport, _ = make_transport(
            lambda _m, _u: (404, {"error": "not_found", "message": "account not found"})
        )
        client = MeterClient("http://engine", transport)
        with self.assertRaises(MeterError) as ctx:
            client.balance("missing")
        self.assertEqual(ctx.exception.status, 404)
        self.assertEqual(ctx.exception.code, "not_found")

    def test_entries_lists_ledger_postings(self) -> None:
        transport, calls = make_transport(
            lambda _m, _u: (
                200,
                [
                    {"id": "le-1", "kind": "grant", "amount": "1000"},
                    {"id": "le-2", "kind": "debit", "amount": "-25"},
                ],
            )
        )
        client = MeterClient("http://engine", transport)
        entries = client.entries("acc-1")
        self.assertEqual([entry["id"] for entry in entries], ["le-1", "le-2"])
        method, url, _body = calls[0]
        self.assertEqual(method, "GET")
        self.assertTrue(url.endswith("/v1/accounts/acc-1/entries"))


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

    def test_langchain_usage(self) -> None:
        usage = langchain_usage(
            {
                "input_tokens": 1000,
                "output_tokens": 500,
                "input_token_details": {"cache_read": 200, "cache_creation": 50},
                "output_token_details": {"reasoning": 120},
            }
        )
        self.assertEqual(
            (
                usage.input_uncached,
                usage.cache_read,
                usage.cache_write,
                usage.output,
                usage.reasoning,
            ),
            (750, 200, 50, 500, 120),
        )

    def test_meter_model_usage_calls_usage_endpoint(self) -> None:
        transport, calls = make_transport(
            lambda _m, _u: (200, {"credits": "52500", "charged": True, "settled": "947500"})
        )
        client = MeterClient("http://engine", transport)
        result = meter_model_usage(
            client,
            org_id="org-1",
            account="acc-1",
            model="claude-opus-4-8",
            idempotency_key="run-1",
            run_id="run-1",
            usage=anthropic_usage({"input_tokens": 1000, "output_tokens": 500}),
        )
        self.assertTrue(result["charged"])
        _method, url, body = calls[0]
        self.assertTrue(url.endswith("/v1/usage"))
        self.assertEqual(body["model"], "claude-opus-4-8")
        self.assertEqual(body["usage"]["input_uncached"], 1000)

    def test_record_model_usage_emits_event(self) -> None:
        transport, calls = make_transport(
            lambda _m, _u: (200, {"id": "evt-1", "status": "recorded"})
        )
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

    def test_metered_call_records_usage_and_returns_response(self) -> None:
        transport, calls = make_transport(
            lambda _m, _u: (200, {"id": "evt-1", "status": "recorded"})
        )
        client = MeterClient("http://engine", transport)
        response = {"usage": {"input_tokens": 1000, "output_tokens": 500}}

        returned = metered_call(
            client,
            org_id="org-1",
            account="acc-1",
            model="claude-opus-4-8",
            idempotency_key="u1",
            extract_usage=lambda r: anthropic_usage(r["usage"]),
            call=lambda: response,
        )

        # The provider response passes through unchanged.
        self.assertIs(returned, response)
        _method, url, body = calls[0]
        self.assertTrue(url.endswith("/v1/events"))
        self.assertEqual(body["properties"]["model"], "claude-opus-4-8")
        self.assertEqual(body["properties"]["input_uncached"], 1000)
        self.assertEqual(body["properties"]["output"], 500)


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
            with_run(
                client, account="acc-1", estimate="40", reservation_id="res-1", work=lambda _s: "x"
            )


if __name__ == "__main__":
    unittest.main()
