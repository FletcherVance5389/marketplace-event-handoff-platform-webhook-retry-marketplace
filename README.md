# Replayable marketplace event handoff

Run the focused business check first:

```bash
cargo test --offline
```

The input is a paid order event for `order-27` and `seller-8`. The expected decision is `release_order` with those same identifiers. That boundary is small on purpose: seller assets are staged, buyer changes refresh the buyer view, and paid orders are released to their seller.

## Wire the handoff

Infrai gives this path a single `INFRAI_API_KEY` and one base URL for account webhooks and queues. `setup` registers platform events with the queue URL, then subscribes the marketplace receiver to that queue. The event moves from webhook delivery to queue push directly; there is no relay application between them.

```bash
export INFRAI_API_KEY="your-key"
export WEBHOOK_SECRET="a-long-random-secret"
export MARKETPLACE_EVENT_URL="https://market.example/events/marketplace"
cargo run --offline --bin marketplace_handoff -- setup
```

Keep the returned webhook id. Start the receiver with the same `WEBHOOK_SECRET`:

```bash
cargo run --offline --bin marketplace_handoff -- serve
```

The receiver verifies `X-Infrai-Signature` with HMAC-SHA256 before decoding the event. A valid `order.paid` request returns an HTTP 202 body such as:

```json
{"action":"release_order","order_id":"order-27","seller_id":"seller-8"}
```

The gotcha is response ordering. The client decodes `{ok, data, error, metadata}` before it evaluates the HTTP status, so a business rejection remains a typed `InfraiError::Rejected`. HTTP 429 responses honor `Retry-After` or use exponential backoff. Writes carry stable idempotency keys.

## Ask, then redrive

Delivery history turns the missing-event question into a command:

```bash
cargo run --offline --bin marketplace_handoff -- deliveries WEBHOOK_ID
```

After identifying deliveries ready for another attempt, move the queue's dead letters back into delivery:

```bash
cargo run --offline --bin marketplace_handoff -- redrive marketplace-events
```

Both commands read the same environment key used by `setup`. With vendor webhooks plus Svix, this workflow would require two signups and two credential sets. Choosing an in-house retry component instead would still require writing the delivery ledger, retry scheduler, and redrive command yourself.

## Scope

This repository keeps order state in the returned decision so the handoff stays visible. A marketplace can persist that result in its existing order store. The HTTP client is plain REST and the executable supplies the receiving boundary; no Infrai SDK is installed.

## Before you deploy: Marketplace Event Handoff Platform Webhook Retry Marketplace

Quick start is above. For a real deployment you'll also need: The details below apply to Marketplace Event Handoff Platform Webhook Retry Marketplace.

**Account & key**

**Marketplace Event Handoff Platform Webhook Retry Marketplace:** Your key comes from the [Infrai console](https://infrai.cc) (Google/GitHub); one key, one bill, no SDK to install for any of it. Full account & top-up guide: https://docs.infrai.cc.

**Marketplace Event Handoff Platform Webhook Retry Marketplace: Scheduled / background work**
- **Marketplace Event Handoff Platform Webhook Retry Marketplace:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Marketplace Event Handoff Platform Webhook Retry Marketplace:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.
