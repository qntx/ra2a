//! Server-side payment gating — the `payment-required` stage of the handshake.
//!
//! [`PaymentGate`] is a server [`CallInterceptor`] that intercepts
//! `message/send` requests for priced skills. A request without a
//! `payment-submitted` payment part is parked: the interceptor short-circuits
//! execution by returning an `ExtensionSupportRequired`-shaped error carrying
//! the serialized v0.1 step-1 payload — a task in
//! [`TaskState::InputRequired`] whose status message holds
//! `x402.payment.status: "payment-required"` plus the `PaymentRequired` offer.
//!
//! Transport-agnostic: the JSON-RPC error string round-trips the full task, so
//! any client that can read an A2A error can recover the offer without
//! touching HTTP specifics.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ra2a::error::A2AError;
use ra2a::server::{CallContext, CallInterceptor, Request};
use ra2a::types::{Message, Part, Task, TaskState, TaskStatus};

use crate::types::{
    KEY_PAYMENT_REQUIRED, KEY_PAYMENT_STATUS, KEY_SKILL_ID, PaymentStatus, SkillPricing,
    X402Extension, meta_get, meta_set,
};

/// Skill id → pricing lookup. Server operators implement this to price
/// requests (typically backed by the agent card's skill metadata).
pub trait PriceLookup: Send + Sync {
    /// Returns the pricing for a skill id, or `None` when the skill is free.
    fn price_for(&self, skill_id: &str) -> Option<SkillPricing>;
}

/// Server interceptor that enforces x402 payment on priced skills.
///
/// Behavior per `message/send`:
/// - If the x402 extension was not activated on the request, the request
///   passes through unchanged (the extension is opt-in per spec §7).
/// - If the message already carries `x402.payment.status: "payment-submitted"`,
///   the request passes through (payment stage is the operator's settlement
///   handler's concern).
/// - If no skill is named (`x402.skill` metadata) or the skill is free, the
///   request passes through.
/// - Otherwise execution short-circuits with a parked `InputRequired` task
///   carrying the `payment-required` offer.
pub struct PaymentGate<P: PriceLookup> {
    /// Skill pricing source (agent-card metadata, DB, etc.).
    pricing: Arc<P>,
    /// `PayTo` address stamped into the offer's requirements (merged into any
    /// requirement missing one).
    pay_to: String,
    /// Resource URL template for the offer (`{skill}` is substituted).
    resource_url: String,
}

impl<P: PriceLookup> std::fmt::Debug for PaymentGate<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaymentGate").finish_non_exhaustive()
    }
}

impl<P: PriceLookup> PaymentGate<P> {
    /// Creates a gate with a pricing source and resource descriptor.
    pub fn new(
        pricing: Arc<P>,
        pay_to: impl Into<String>,
        resource_url: impl Into<String>,
    ) -> Self {
        Self {
            pricing,
            pay_to: pay_to.into(),
            resource_url: resource_url.into(),
        }
    }

    /// Builds the v0.1 `x402.payment.required` value for a skill: an
    /// `x402Version: 2` offer whose `accepts` come verbatim from the skill's
    /// pricing (wire camelCase), plus `payTo`/`resource` backfills where the
    /// operator left them out.
    #[must_use]
    pub fn build_offer(&self, skill_id: &str, pricing: &SkillPricing) -> serde_json::Value {
        let resource = format!(
            "{}/skills/{}",
            self.resource_url.trim_end_matches('/'),
            skill_id
        );
        let accepts: Vec<serde_json::Value> = pricing
            .requirements
            .iter()
            .map(|r| {
                let mut obj = r.clone();
                if let Some(map) = obj.as_object_mut() {
                    map.entry(String::from("payTo"))
                        .or_insert_with(|| serde_json::json!(self.pay_to));
                    map.entry(String::from("resource"))
                        .or_insert_with(|| serde_json::json!(resource));
                }
                obj
            })
            .collect();

        let mut offer = serde_json::json!({ "x402Version": 2, "accepts": accepts });
        if let (Some(map), Some(desc)) = (offer.as_object_mut(), &pricing.description) {
            map.insert(String::from("description"), serde_json::json!(desc));
        }
        offer
    }
}

impl<P: PriceLookup + 'static> CallInterceptor for PaymentGate<P> {
    fn before<'a>(
        &'a self,
        ctx: &'a mut CallContext,
        req: &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = Result<(), A2AError>> + Send + 'a>> {
        Box::pin(async move {
            // Only guard message/send payloads.
            let Some(params) = req.downcast_ref::<ra2a::types::SendMessageRequest>() else {
                return Ok(());
            };
            let message = &params.message;

            // Activated? (opt-in extension per spec §7) — either requested via
            // the `X-A2A-Extensions` header or activated in this call scope.
            if !ctx
                .requested_extension_uris()
                .iter()
                .any(|u| u == X402Extension::uri())
                && !ctx.is_extension_active(X402Extension::uri())
            {
                return Ok(());
            }

            // Already paying? The settlement stage owns that path.
            if let Some(status) = meta_get(message.metadata.as_ref(), KEY_PAYMENT_STATUS)
                && status.as_str() == Some(PaymentStatus::Submitted.as_str())
            {
                return Ok(());
            }

            // Skill named + priced?
            let Some(skill_id) = message
                .metadata
                .as_ref()
                .and_then(|m| m.get(KEY_SKILL_ID))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
            else {
                return Ok(());
            };
            let Some(pricing) = self.pricing.price_for(&skill_id) else {
                return Ok(()); // free skill
            };

            // Build the parked task: InputRequired + payment-required offer.
            let offer = self.build_offer(&skill_id, &pricing);
            let mut reply = Message::agent(vec![Part::text(format!(
                "Payment is required for skill '{skill_id}'."
            ))]);
            meta_set(
                &mut reply.metadata,
                KEY_PAYMENT_STATUS,
                serde_json::json!(PaymentStatus::Required.as_str()),
            );
            meta_set(&mut reply.metadata, KEY_PAYMENT_REQUIRED, offer);

            let task_id = message
                .task_id
                .clone()
                .unwrap_or_else(ra2a::types::TaskId::random);
            let context_id = message
                .context_id
                .clone()
                .unwrap_or_else(|| ra2a::types::ContextId::random().to_string());
            let mut task = Task::new(task_id, context_id);
            task.status = TaskStatus::with_message(TaskState::InputRequired, reply);

            // Short-circuit with the task serialized into the error payload.
            let body = serde_json::to_value(&task)
                .map_err(|e| A2AError::InternalError(format!("x402: serialize task: {e}")))?;
            Err(A2AError::ServerError(serde_json::to_string(
                &serde_json::json!({
                    "x402": true,
                    "status": PaymentStatus::Required.as_str(),
                    "task": body,
                }),
            )?))
        })
    }

    fn after<'a>(
        &'a self,
        _ctx: &'a CallContext,
        _resp: &'a mut ra2a::server::Response,
    ) -> Pin<Box<dyn Future<Output = Result<(), A2AError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

/// Helper for settlement handlers: extracts a parked-task id from a
/// `payment-submitted` message (the spec requires the client to echo the
/// original task id via `Message.task_id`).
#[must_use]
pub fn submitted_task_id(message: &Message) -> Option<&ra2a::types::TaskId> {
    let status = message
        .metadata
        .as_ref()
        .and_then(|m| m.get(KEY_PAYMENT_STATUS))?;
    if status.as_str() == Some(PaymentStatus::Submitted.as_str()) {
        message.task_id.as_ref()
    } else {
        None
    }
}

/// Convenience for operators: attaches pricing to the gate from a
/// skill-id → requirements map (the common card-metadata shape).
#[derive(Debug)]
pub struct MapPricing(pub HashMap<String, SkillPricing>);

// HashMap key is the skill id (`x402.skill` metadata value).

impl PriceLookup for MapPricing {
    fn price_for(&self, skill_id: &str) -> Option<SkillPricing> {
        self.0.get(skill_id).cloned()
    }
}
