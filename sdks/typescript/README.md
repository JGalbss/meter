# meter — TypeScript SDK

A thin, drop-in client for the meter engine, plus adapters that normalize usage from the major AI
providers (Anthropic / Claude, OpenAI, Google Gemini, AWS Bedrock, LangChain, the Vercel AI SDK) and
helpers that govern a run (reserve → settle → auto-void).

```bash
npm install @meter/sdk   # or: pnpm add @meter/sdk
```

```ts
import { MeterClient, anthropicUsage, meterModelUsage, withRun } from "@meter/sdk"

const meter = new MeterClient({ baseUrl: "http://localhost:8080" })

// The core loop: price provider usage into credits, record the event, and charge — one idempotent call.
const result = await meterModelUsage(meter, {
  orgId,
  account,
  model: "claude-opus-4-8",
  idempotencyKey: requestId,
  usage: anthropicUsage(response.usage), // normalize the provider's token counts
})
```

## Govern a run (reserve → settle / void)

`withRun` reserves a worst-case estimate before the work runs; if the reservation is denied the work
never starts. Settle the actual usage via the handle — if `work` throws or never settles, the hold is
voided so a failed run leaves no lingering reservation.

```ts
await withRun(meter, { account, estimate: "40" }, async (run) => {
  const completion = await callTheModel()
  const { credits } = await meterModelUsage(meter, {
    orgId,
    account,
    model: "claude-opus-4-8",
    idempotencyKey: completion.id,
    usage: anthropicUsage(completion.usage),
  })
  await run.settle(credits)
})
```

Prefer to govern in **token terms**? `withRunUsage` reserves a hold sized to an estimated token usage
for a model (the engine prices it) and settles with the actual tokens — same auto-void-on-failure:

```ts
import { withRunUsage } from "@meter/sdk"

await withRunUsage(
  meter,
  { account, model: "claude-opus-4-8", estimate: { input_uncached: 4000, output: 1000 } },
  async (run) => {
    const completion = await callTheModel()
    await run.settle(anthropicUsage(completion.usage))
  },
)
```

## Provider adapters

Each maps a provider's usage object to meter's normalized token dimensions:
`anthropicUsage`, `openaiUsage`, `geminiUsage`, `bedrockUsage`, `langchainUsage`, `vercelAiUsage` —
plus `meterModelUsage` (price + charge), `recordModelUsage` (emit a usage event only), and
`meteredCall` (wrap a provider call, record its usage, and return the response unchanged).

## `MeterClient`

`openAccount`, `balance`, `grant`, `entries`, `reserve`, `settle`, `extendReservation`,
`voidReservation`, `openLease`, `closeLease`, `recordEvent`, `amendEvent`, `listEvents`, `voidRun`,
`invoice`, `meterUsage`, `reserveUsage`, `settleUsage`, `catalog`, `simulate`.

`catalog` lists the hosted model prices and `simulate` re-rates a usage stream across two models —
useful for cost-aware routing and budgeting from your own code.

`reserve` accepts an optional `expiresAt` (RFC3339) hold timeout; `extendReservation` pushes it out — a
heartbeat so a long-running reservation isn't swept.

`reserveUsage` / `settleUsage` are the **token-priced** two-phase flow: reserve a hold sized to an
estimated token usage for a model (the engine prices it), then settle with the actuals — enforcement in
token terms rather than raw credits.

Per-session **leasing** (`openLease` / `closeLease`) funds a child account from a parent once and spends
locally, avoiding a ledger round-trip per call — see `docs/adr/0005-provider-scale-throughput.md`.

## Notes

The hand-written client will be replaced by a **Stainless-generated** client from the engine OpenAPI
spec once it is emitted (see `docs/SDKS.md`); the adapters and run governance carry over. Tests:
`pnpm --filter @meter/sdk run test`.
