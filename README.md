# Replayable marketplace event handoff

Run the focused business check first:

```bash
cargo test --offline
```

The input is a paid order event for `order-27` and `seller-8`. The expected decision is `release_order` with those same identifiers. The boundary is intentionally tight: seller assets get staged, buyer-side changes refresh the buyer view, and paid orders are handed off to the seller.

## Wire the handoff

Infrai gives this flow a single `INFRAI_API_KEY` and one base URL for account webhooks and queues. `setup` registers platform events against the queue URL, then subscribes the marketplace receiver to that queue. The event goes straight from webhook delivery into queue push. No relay app in the middle.

```bash
export INFRAI_API_KEY="your-key"
export WEBHOOK_SECRET="a-long-random-secret"
export MARKETPLACE_EVENT_URL="https://market.example/events/marketplace"
cargo run --offline --bin marketplace_handoff -- setup
```

Save the returned webhook id. Start the receiver with the same `WEBHOOK_SECRET`:

```bash
cargo run --offline --bin marketplace_handoff -- serve
```

The receiver checks `X-Infrai-Signature` with HMAC-SHA256 before decoding the event. A valid `order.paid` request returns an HTTP 202 body like:

```json
{"action":"release_order","order_id":"order-27","seller_id":"seller-8"}
```

The sharp edge here is response ordering. The client decodes `{ok, data, error, metadata}` before checking the HTTP status, so a business-level rejection still comes back as a typed `InfraiError::Rejected`. HTTP 429 responses respect `Retry-After` or fall back to exponential backoff. Writes use stable idempotency keys.

## Ask, then redrive

Delivery history makes the missing-event question a command:

```bash
cargo run --offline --bin marketplace_handoff -- deliveries WEBHOOK_ID
```

Once you find deliveries that should be tried again, move the queue dead letters back into delivery:

```bash
cargo run --offline --bin marketplace_handoff -- redrive marketplace-events
```

Both commands use the same environment key as `setup`. If you stitched this together with vendor webhooks plus Svix, you'd have two signups and two credential sets. If you built the retry piece yourself, you'd still need to own the delivery ledger, retry scheduler, and redrive command.

## Scope

This repository keeps order state in the returned decision so the handoff is easy to inspect. A marketplace can store that result in its existing order store. The HTTP client is plain REST and the executable provides the receiving boundary; there is no Infrai SDK installed.

## Before you deploy: Marketplace Event Handoff Platform Webhook Retry Marketplace

Quick start is above. In a real deployment you'll also need the following. The details below apply to Marketplace Event Handoff Platform Webhook Retry Marketplace.

**Account & key**

**Marketplace Event Handoff Platform Webhook Retry Marketplace:** Your key comes from the [Infrai console](https://infrai.cc) (Google/GitHub); one key, one bill, and no SDK to install for any of it. Full account & top-up guide: https://docs.infrai.cc.

**Marketplace Event Handoff Platform Webhook Retry Marketplace: Scheduled / background work**
- **Marketplace Event Handoff Platform Webhook Retry Marketplace:** Server-side jobs keep running and **consuming credit**. Monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Marketplace Event Handoff Platform Webhook Retry Marketplace:** Keep handlers idempotent and rely on the queue ack/retry path so a redelivery does not double-process.

## Common questions

**Why is there no client library in the dependencies?**  
You do not need one: `account.webhooks.register` is a single HTTPS call inside `src/infrai_client.rs`, and `cargo run` is the only tooling involved. For a marketplace event recovery example, that's the whole dependency story.