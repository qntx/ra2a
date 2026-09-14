# ra2a-ext-x402

[![Crates.io](https://img.shields.io/crates/v/ra2a-ext-x402.svg)](https://crates.io/crates/ra2a-ext-x402)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT_OR_Apache_2.0-blue.svg)](../LICENSE-MIT)

x402 payments extension for the [A2A Rust SDK (`ra2a`)](https://crates.io/crates/ra2a),
implementing the [google-agentic-commerce/a2a-x402 v0.1](https://github.com/google-agentic-commerce/a2a-x402)
message-level handshake:

```
payment-required → payment-submitted → payment-completed
```

Payment state rides in A2A `Message.metadata` under the `x402.payment.*` keys defined
by the spec. The `payment-required` stage maps onto ra2a's existing
`TaskState::InputRequired` interactive state — the task resumes to `Working` once the
client submits payment — so the payments layer composes with the task state machine
instead of fighting it. No scheme logic is duplicated: the offer/payload JSON is the
wire shape any x402 V2 client already produces.

## Quick start

### Server — gate priced skills

```rust
use ra2a_ext_x402::{MapPricing, PaymentGate, SkillPricing};
use std::{collections::HashMap, sync::Arc};

let mut pricing = HashMap::new();
pricing.insert(
    "generate-image".to_string(),
    SkillPricing {
        requirements: vec![serde_json::json!({
            "scheme": "exact",
            "network": "eip155:8453",
            "amount": "10000",
            "asset": "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            "extra": { "name": "USD Coin", "version": "2" }
        })],
        description: Some("Generate an image".into()),
    },
);

let gate = PaymentGate::new(
    Arc::new(MapPricing(pricing)),
    "0xYourPayToAddress",
    "https://your-agent.example",
);
server.middleware(gate);
```

A `message/send` for a priced skill without payment parks the task in
`InputRequired` with a `payment-required` offer in the status message metadata.
Operators can implement [`PriceLookup`] to source pricing from anywhere
(agent card metadata, a database, a billing service).

### Client — sign and resubmit

```rust
use ra2a_ext_x402::{PaymentClient, PaymentSigner};

struct WalletSigner { /* your x402 client + wallet */ }

impl PaymentSigner for WalletSigner {
    async fn sign(&self, offer: &serde_json::Value) -> Result<serde_json::Value, String> {
        // pick an `accepts` entry, authorize, return the wire PaymentPayload
        # unimplemented!()
    }
}

client.interceptor(PaymentClient::new(Arc::new(WalletSigner { /* .. */ })));
```

The interceptor sees a mirrored pending offer on the outgoing message, signs it
through your wallet stack, and rewrites the message into the `payment-submitted`
resubmission (same task id). For manual orchestration, [`submitted_message`] builds
that message directly.

### Declaring the extension

```rust
use ra2a_ext_x402::X402Extension;

X402Extension::declare_on_card(&mut card);
```

Per a2a-x402 §7 the extension is opt-in: clients activate it via
`X-A2A-Extensions: https://github.com/aspect-build/a2a-x402/uri` (v0.1 URI carried
by [`X402_EXTENSION_URI`]); the server gate only charges requests where the client
activated the extension.

## Extension points

| Type | Role |
|---|---|
| [`PaymentGate`] | Server intercept: park unpaid priced requests with an offer |
| [`PriceLookup`] / [`MapPricing`] | Where pricing comes from (any source) |
| [`PaymentClient`] + [`PaymentSigner`] | Client intercept: sign offers via any wallet/x402 stack |
| [`submitted_task_id`] | Settlement helper: correlate the resubmission to its parked task |

## Tested

8 integration-style tests cover the full handshake: parking (offer validity,
payTo/resource stamping, `InputRequired` state), pass-through for unactivated
clients / free skills / already-paid requests, card declaration idempotency and
`required` flag, resubmission correlation, and status vocabulary round-trips.

Run with `cargo test -p ra2a-ext-x402 --all-features`.

## License

MIT OR Apache-2.0 — matches the parent workspace.

<!-- crate-doc anchors -->
[`PriceLookup`]: https://docs.rs/ra2a
[`submitted_message`]: https://docs.rs/ra2a