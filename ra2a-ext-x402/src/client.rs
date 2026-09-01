//! Client-side payment handling — the `payment-submitted` stage of the handshake.
//!
//! [`PaymentClient`] is a client [`CallInterceptor`] that, before sending,
//! checks the pending offer mirrored onto the outgoing message and — when
//! present — invokes the operator's signing callback to produce a signed
//! payment payload and rewrites the message into the `payment-submitted`
//! resubmission (spec v0.1 step 3): same task id, metadata key
//! `x402.payment.payload` set.
//!
//! For manual orchestration, [`submitted_message`] builds the resubmission
//! message directly from a signed payload (the spec requires the original
//! task id on the message).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ra2a::client::{CallInterceptor, Request};
use ra2a::error::{A2AError, Result};
use ra2a::types::{Message, Part};

use crate::types::{
    KEY_PAYMENT_PAYLOAD, KEY_PAYMENT_REQUIRED, KEY_PAYMENT_STATUS, PaymentStatus, meta_get,
    meta_set,
};

/// Signs a `payment-required` offer into a wire payload.
///
/// Implemented over the x402 client stack (e.g. `r402`'s exact/upto schemes +
/// a wallet); kept as a trait so the crate stays chain-agnostic. The returned
/// value is the wire `PaymentPayload` (camelCase), including the `accepted`
/// requirements the buyer chose.
pub trait PaymentSigner: Send + Sync {
    /// Selects one of the offered requirements, signs it, and returns the
    /// signed wire payload. Return `Err` to skip payment — the request then
    /// proceeds unmodified and the caller surfaces the parked task to the user.
    fn sign(
        &self,
        offer: &serde_json::Value,
    ) -> impl Future<Output = std::result::Result<serde_json::Value, String>> + Send;
}

/// Client interceptor implementing the buyer side of the v0.1 flow.
pub struct PaymentClient<S: PaymentSigner> {
    /// The wallet/signing stack used to authorize offers.
    signer: Arc<S>,
}

impl<S: PaymentSigner> std::fmt::Debug for PaymentClient<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaymentClient").finish_non_exhaustive()
    }
}

impl<S: PaymentSigner> PaymentClient<S> {
    /// Creates a client interceptor with the given signer.
    pub const fn new(signer: Arc<S>) -> Self {
        Self { signer }
    }
}

impl<S: PaymentSigner + 'static> CallInterceptor for PaymentClient<S> {
    fn before<'a>(
        &'a self,
        req: &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let Some(params) = req
                .payload
                .downcast_mut::<ra2a::types::SendMessageRequest>()
            else {
                return Ok(());
            };

            // If we're already submitting payment, nothing to do.
            if let Some(status) = meta_get(params.message.metadata.as_ref(), KEY_PAYMENT_STATUS)
                && status.as_str() == Some(PaymentStatus::Submitted.as_str())
            {
                return Ok(());
            }

            // The pending offer must be mirrored onto the outgoing message
            // metadata (from the parked task the client received).
            let Some(offer_value) =
                meta_get(params.message.metadata.as_ref(), KEY_PAYMENT_REQUIRED).cloned()
            else {
                return Ok(());
            };

            // Sign via the operator's wallet stack.
            let payload = self
                .signer
                .sign(&offer_value)
                .await
                .map_err(|e| A2AError::Other(format!("x402: signing declined: {e}")))?;

            // Rewrite the message into a payment-submitted resubmission.
            meta_set(
                &mut params.message.metadata,
                KEY_PAYMENT_STATUS,
                serde_json::json!(PaymentStatus::Submitted.as_str()),
            );
            meta_set(&mut params.message.metadata, KEY_PAYMENT_PAYLOAD, payload);
            params
                .message
                .parts
                .push(Part::text("Payment authorization attached."));

            Ok(())
        })
    }
}

/// Builds a `payment-submitted` resubmission message for a task from a signed
/// wire payload. The message carries the original task id (spec §4.5: the
/// server correlates the payment via `taskId`).
#[must_use]
pub fn submitted_message(
    task_id: impl Into<ra2a::types::TaskId>,
    payload: serde_json::Value,
) -> Message {
    let mut message = Message::user(vec![Part::text("Here is the payment authorization.")]);
    message.task_id = Some(task_id.into());
    meta_set(
        &mut message.metadata,
        KEY_PAYMENT_STATUS,
        serde_json::json!(PaymentStatus::Submitted.as_str()),
    );
    meta_set(&mut message.metadata, KEY_PAYMENT_PAYLOAD, payload);
    message
}
