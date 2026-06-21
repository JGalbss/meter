"""Tests for the auto-patch wrappers (unittest, fake transport + fake providers)."""

from __future__ import annotations

import json
import unittest
from types import SimpleNamespace
from typing import Any

from meter import MeterClient, patch_anthropic, patch_openai


def make_transport(status: int = 200, payload: Any | None = None):
    calls: list[tuple[str, str, Any]] = []
    body_payload = payload or {
        "credits": "52500",
        "charged": True,
        "settled": "947500",
        "id": "evt-1",
        "status": "recorded",
    }

    def transport(method, url, body, headers):
        decoded = None if body is None else json.loads(body.decode())
        calls.append((method, url, decoded))
        return status, json.dumps(body_payload).encode()

    return transport, calls


def fake_anthropic(response: Any):
    invocations: list[dict[str, Any]] = []

    def create(*_args: Any, **kwargs: Any) -> Any:
        invocations.append(kwargs)
        return response

    provider = SimpleNamespace(messages=SimpleNamespace(create=create))
    return provider, invocations


def fake_openai(response: Any):
    def create(*_args: Any, **_kwargs: Any) -> Any:
        return response

    return SimpleNamespace(chat=SimpleNamespace(completions=SimpleNamespace(create=create)))


class PatchAnthropicTests(unittest.TestCase):
    def test_meters_each_call_and_returns_response_unchanged(self) -> None:
        transport, calls = make_transport()
        client = MeterClient("http://engine", transport)
        response = SimpleNamespace(
            model="claude-opus-4-8",
            usage={"input_tokens": 1000, "output_tokens": 500, "cache_read_input_tokens": 200},
            content="hi",
        )
        provider, _ = fake_anthropic(response)

        unpatch = patch_anthropic(client, provider, org_id="org-1", account="acc-1")
        returned = provider.messages.create(model="claude-opus-4-8")

        self.assertIs(returned, response)
        self.assertEqual(len(calls), 1)
        _method, url, body = calls[0]
        self.assertTrue(url.endswith("/v1/usage"))
        self.assertEqual(body["model"], "claude-opus-4-8")
        self.assertEqual(body["usage"]["input_uncached"], 1000)
        self.assertEqual(body["usage"]["cache_read"], 200)
        self.assertIsInstance(body["idempotency_key"], str)

        unpatch()
        provider.messages.create(model="claude-opus-4-8")
        self.assertEqual(len(calls), 1)  # no metering after unpatch

    def test_fresh_idempotency_key_per_call(self) -> None:
        transport, calls = make_transport()
        client = MeterClient("http://engine", transport)
        response = SimpleNamespace(model="m", usage={"input_tokens": 1, "output_tokens": 1})
        provider, _ = fake_anthropic(response)

        patch_anthropic(client, provider, org_id="org-1", account="acc-1")
        provider.messages.create()
        provider.messages.create()
        self.assertEqual(len(calls), 2)
        self.assertNotEqual(calls[0][2]["idempotency_key"], calls[1][2]["idempotency_key"])

    def test_falls_back_to_request_model(self) -> None:
        transport, calls = make_transport()
        client = MeterClient("http://engine", transport)
        response = {"usage": {"input_tokens": 1, "output_tokens": 1}}  # no model on the response
        provider, _ = fake_anthropic(response)

        patch_anthropic(client, provider, org_id="org-1", account="acc-1")
        provider.messages.create(model="claude-haiku-4-5")
        self.assertEqual(calls[0][2]["model"], "claude-haiku-4-5")

    def test_skips_a_response_without_usage(self) -> None:
        transport, calls = make_transport()
        client = MeterClient("http://engine", transport)
        provider, _ = fake_anthropic(SimpleNamespace(model="m"))

        patch_anthropic(client, provider, org_id="org-1", account="acc-1")
        provider.messages.create()
        self.assertEqual(len(calls), 0)

    def test_preserves_self_binding(self) -> None:
        transport, calls = make_transport()
        client = MeterClient("http://engine", transport)

        # A class whose create() reads instance state via `self` — the patch must keep it bound.
        class Messages:
            default_model = "claude-opus-4-8"

            def create(self) -> Any:
                return SimpleNamespace(
                    model=self.default_model,
                    usage={"input_tokens": 1, "output_tokens": 1},
                )

        provider = SimpleNamespace(messages=Messages())
        patch_anthropic(client, provider, org_id="org-1", account="acc-1")
        response = provider.messages.create()

        self.assertEqual(response.model, "claude-opus-4-8")  # self.default_model resolved
        self.assertEqual(calls[0][2]["model"], "claude-opus-4-8")


class PatchOpenAiTests(unittest.TestCase):
    def test_record_mode_emits_event_and_coerces_object_usage(self) -> None:
        transport, calls = make_transport()
        client = MeterClient("http://engine", transport)
        # usage is an object (pydantic-like), exercising the dict coercion path.
        response = SimpleNamespace(
            model="gpt-4o",
            usage=SimpleNamespace(
                prompt_tokens=1000,
                completion_tokens=500,
                prompt_tokens_details={"cached_tokens": 200},
            ),
        )
        provider = fake_openai(response)

        patch_openai(
            client,
            provider,
            org_id="org-1",
            account="acc-1",
            mode="record",
            extra={"team": "research"},
        )
        provider.chat.completions.create()

        self.assertEqual(len(calls), 1)
        _method, url, body = calls[0]
        self.assertTrue(url.endswith("/v1/events"))
        props = body["properties"]
        self.assertEqual(props["model"], "gpt-4o")
        self.assertEqual(props["input_uncached"], 800)
        self.assertEqual(props["cache_read"], 200)
        self.assertEqual(props["team"], "research")


class PatchErrorHandlingTests(unittest.TestCase):
    def test_rethrows_without_on_error(self) -> None:
        transport, _ = make_transport(status=500, payload={"error": "boom"})
        client = MeterClient("http://engine", transport)
        response = SimpleNamespace(model="m", usage={"input_tokens": 1, "output_tokens": 1})
        provider, _ = fake_anthropic(response)

        patch_anthropic(client, provider, org_id="org-1", account="acc-1")
        with self.assertRaises(Exception):
            provider.messages.create()

    def test_fail_open_with_on_error(self) -> None:
        transport, _ = make_transport(status=500, payload={"error": "boom"})
        client = MeterClient("http://engine", transport)
        response = SimpleNamespace(
            model="m", usage={"input_tokens": 1, "output_tokens": 1}, ok=True
        )
        provider, _ = fake_anthropic(response)

        captured: list[BaseException] = []
        patch_anthropic(
            client,
            provider,
            org_id="org-1",
            account="acc-1",
            on_error=captured.append,
        )
        returned = provider.messages.create()
        self.assertTrue(returned.ok)
        self.assertEqual(len(captured), 1)


if __name__ == "__main__":
    unittest.main()
