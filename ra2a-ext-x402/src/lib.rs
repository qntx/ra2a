//! # ra2a-ext-x402
//!
//! x402 payments extension for the [A2A Rust SDK](https://crates.io/crates/ra2a),
//! implementing the [google-agentic-commerce/a2a-x402 v0.1](https://github.com/google-agentic-commerce/a2a-x402)
//! message-level handshake: `payment-required` → `payment-submitted` → `payment-completed`.
//!
//! Payment state rides in A2A `Message.metadata` under the `x402.payment.*`
//! keys defined by the spec. The payment request maps onto the task lifecycle
//! via [`ra2a::types::TaskState::InputRequired`] (the task resumes to `Working`
//! once the client submits payment), composing with the existing task state
//! machine rather than fighting it. Payment wire types are plain JSON in the
//! shapes the a2a-x402 v0.1 spec (and any x402 V2 client) already produces —
//! no scheme logic is duplicated here.
//!
//! ## Components
//!
//! - **[`X402Extension`]** — the canonical extension URI plus agent-card
//!   declaration.
//! - **[`PaymentGate`]** — server-side [`CallInterceptor`](ra2a::server::CallInterceptor):
//!   parks `message/send` requests for priced skills (via [`PriceLookup`] /
//!   [`MapPricing`]) in `InputRequired` with a `payment-required` offer.
//! - **[`submitted_task_id`]** — server helper for the settlement stage:
//!   correlates a `payment-submitted` message back to its task.
//! - **[`PaymentClient`]** — client-side [`CallInterceptor`](ra2a::client::CallInterceptor):
//!   signs a pending offer via [`PaymentSigner`] and rewrites the outgoing
//!   message into the `payment-submitted` resubmission. [`submitted_message`]
//!   builds that message for manual orchestration.
//!
//! ## Spec mapping (v0.1)
//!
//! | Spec artifact | Crate location |
//! |---|---|
//! | Extension URI + card declaration | [`X402Extension`] |
//! | `x402.payment.status` values | [`PaymentStatus`] |
//! | `x402PaymentRequiredResponse` (`x402.payment.required`) | [`PaymentGate::build_offer`] |
//! | `PaymentPayload` (`x402.payment.payload`) | [`PaymentSigner`] / [`submitted_message`] |
//! | `x402SettleResponse` (receipts) | [`Receipt`] |

#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

mod client;
mod server;
#[cfg(test)]
mod tests;
mod types;

pub use client::{PaymentClient, PaymentSigner, submitted_message};
pub use server::{MapPricing, PaymentGate, PriceLookup, submitted_task_id};
pub use types::{
    KEY_PAYMENT_ERROR, KEY_PAYMENT_PAYLOAD, KEY_PAYMENT_RECEIPTS, KEY_PAYMENT_REQUIRED,
    KEY_PAYMENT_STATUS, KEY_SKILL_ID, PaymentStatus, Receipt, SkillPricing, X402_EXTENSION_URI,
    X402_METADATA_PREFIX, X402Extension,
};
