# meter — Python SDK

Standard-library-only client for the meter engine, plus adapters that auto-instrument the major AI
clients (Anthropic / Claude + Agent SDK, OpenAI) to emit usage.

```python
from meter import MeterClient, anthropic_usage, record_model_usage, with_run

meter = MeterClient("http://localhost:8080")

# Emit usage from a provider response (OpenTelemetry-style):
record_model_usage(
    meter,
    org_id=org, account=account, model="claude-opus-4-8", idempotency_key=request_id, run_id=run,
    usage=anthropic_usage(response.usage),
)

# Govern a run: reserve up front, settle actuals, auto-void on failure:
def work(settle):
    result = call_the_model()
    settle("30")  # actual credits
    return result

with_run(meter, account=account, estimate="40", work=work)
```

The base client will be replaced by a **Stainless-generated** client from the engine OpenAPI spec
(see `docs/SDKS.md`); the adapters and run governance carry over. Tests: `PYTHONPATH=. python3 -m unittest`.
