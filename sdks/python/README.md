# meter — Python SDK

A standard-library-only client for the meter engine, plus adapters that normalize usage from the major
AI providers (Anthropic / Claude, OpenAI, Google Gemini, AWS Bedrock, LangChain) and helpers that
govern a run (reserve → settle → auto-void). No third-party dependencies.

```bash
pip install meter-sdk
```

```python
from meter import MeterClient, anthropic_usage, meter_model_usage

meter = MeterClient("http://localhost:8080")

# The core loop: price provider usage into credits, record the event, and charge — one idempotent call.
result = meter_model_usage(
    meter,
    org_id=org_id,
    account=account,
    model="claude-opus-4-8",
    idempotency_key=request_id,
    usage=anthropic_usage(response.usage),  # normalize the provider's token counts
)
```

## Govern a run (reserve → settle / void)

`with_run` reserves a worst-case estimate before the work runs; if the reservation is denied the work
never starts. Settle the actual usage via the `settle` callback — if `work` raises or never settles,
the hold is voided so a failed run leaves no lingering reservation.

```python
def work(settle):
    completion = call_the_model()
    result = meter_model_usage(
        meter,
        org_id=org_id,
        account=account,
        model="claude-opus-4-8",
        idempotency_key=completion.id,
        usage=anthropic_usage(completion.usage),
    )
    settle(result["credits"])  # actual credits charged
    return completion

with_run(meter, account=account, estimate="40", work=work)
```

## Provider adapters

Each maps a provider's usage object to meter's normalized token dimensions:
`anthropic_usage`, `openai_usage`, `gemini_usage`, `bedrock_usage`, `langchain_usage` — plus
`meter_model_usage` (price + charge), `record_model_usage` (emit a usage event only), and
`metered_call` (run a provider call, record its usage, and return the response unchanged).

```python
from meter import anthropic_usage, metered_call

# Wrap an existing call site; the provider response passes through unchanged.
response = metered_call(
    meter,
    org_id=org_id,
    account=account,
    model="claude-opus-4-8",
    idempotency_key=request_id,
    extract_usage=lambda r: anthropic_usage(r.usage),
    call=lambda: client.messages.create(...),
)
```

## `MeterClient`

`open_account`, `balance`, `grant`, `entries`, `reserve`, `settle`, `extend_reservation`,
`void_reservation`, `open_lease`, `close_lease`, `record_event`, `amend_event`, `list_events`,
`void_run`, `invoice`, `meter_usage`, `reserve_usage`, `settle_usage`.

`reserve` accepts an optional `expires_at` (RFC3339) hold timeout; `extend_reservation` pushes it out — a
heartbeat so a long-running reservation isn't swept.

`reserve_usage` / `settle_usage` are the **token-priced** two-phase flow: reserve a hold sized to an
estimated token usage for a model (the engine prices it), then settle with the actuals — enforcement in
token terms rather than raw credits.

Per-session **leasing** (`open_lease` / `close_lease`) funds a child account from a parent once and
spends locally, avoiding a ledger round-trip per call — see
`docs/adr/0005-provider-scale-throughput.md`.

## Notes

The base client will be replaced by a **Stainless-generated** client from the engine OpenAPI spec once
it is emitted (see `docs/SDKS.md`); the adapters and run governance carry over. The HTTP transport is
injectable, so tests run without the network: `PYTHONPATH=. python3 -m unittest`.
