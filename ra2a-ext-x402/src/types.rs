//! Shared types for the x402 A2A extension — spec v0.1 vocabulary.

use serde_json::Value;

/// Canonical extension URI for a2a-x402 v0.1. Implementations MUST use this
/// URI for declaration and activation.
pub const X402_EXTENSION_URI: &str = "https://github.com/google-a2a/a2a-x402/v0.1";

/// Metadata key prefix used by the spec. All payment state rides in
/// `Message.metadata` under `x402.payment.*`.
pub const X402_METADATA_PREFIX: &str = "x402.payment.";

/// Full metadata key for the payment status field.
pub const KEY_PAYMENT_STATUS: &str = "x402.payment.status";
/// Metadata key carrying the `PaymentRequired` offer (`x402.payment.required`).
pub const KEY_PAYMENT_REQUIRED: &str = "x402.payment.required";
/// Metadata key carrying the signed `PaymentPayload` (`x402.payment.payload`).
pub const KEY_PAYMENT_PAYLOAD: &str = "x402.payment.payload";
/// Metadata key carrying the settlement receipt history (`x402.payment.receipts`).
pub const KEY_PAYMENT_RECEIPTS: &str = "x402.payment.receipts";
/// Metadata key carrying a failure reason code (`x402.payment.error`).
pub const KEY_PAYMENT_ERROR: &str = "x402.payment.error";

/// Message metadata key naming the skill being billed (`x402.skill`).
pub const KEY_SKILL_ID: &str = "x402.skill";

/// Payment lifecycle states from the v0.1 spec (`x402.payment.status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentStatus {
    /// Requirements have been sent to the client agent.
    Required,
    /// Client rejected the requirements.
    Rejected,
    /// Signed payload has been received by the server agent.
    Submitted,
    /// Payload has been verified by the server agent.
    Verified,
    /// Transaction has settled on-chain.
    Completed,
    /// Verification, settlement, or on-chain posting failed.
    Failed,
}

impl PaymentStatus {
    /// The spec wire string for this status.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Required => "payment-required",
            Self::Rejected => "payment-rejected",
            Self::Submitted => "payment-submitted",
            Self::Verified => "payment-verified",
            Self::Completed => "payment-completed",
            Self::Failed => "payment-failed",
        }
    }

    /// Parses a spec wire string back to a status.
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "payment-required" => Some(Self::Required),
            "payment-rejected" => Some(Self::Rejected),
            "payment-submitted" => Some(Self::Submitted),
            "payment-verified" => Some(Self::Verified),
            "payment-completed" => Some(Self::Completed),
            "payment-failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Declares the x402 extension on an agent card and checks activation.
///
/// Wraps the canonical URI so callers don't hand-type it.
#[derive(Debug, Clone, Copy, Default)]
pub struct X402Extension;

impl X402Extension {
    /// The canonical v0.1 extension URI.
    #[must_use]
    pub const fn uri() -> &'static str {
        X402_EXTENSION_URI
    }

    /// Marks the extension as supported (with `required: true`, per the
    /// spec's recommendation) in an agent card's `capabilities.extensions`.
    ///
    /// No-ops if the URI is already declared.
    pub fn declare_on_card(card: &mut ra2a::types::AgentCard) {
        let already = card
            .capabilities
            .extensions
            .iter()
            .any(|e| e.uri == X402_EXTENSION_URI);
        if already {
            return;
        }
        card.capabilities
            .extensions
            .push(ra2a::types::AgentExtension {
                uri: String::from(X402_EXTENSION_URI),
                description: Some(String::from(
                    "Supports payments using the `x402` protocol for on-chain settlement.",
                )),
                required: true,
                params: None,
            });
    }
}

/// Reads a metadata key from a message's metadata map.
#[must_use]
pub(crate) fn meta_get<'m>(
    metadata: Option<&'m ra2a::types::Metadata>,
    key: &str,
) -> Option<&'m Value> {
    metadata.and_then(|m| m.get(key))
}

/// Sets a metadata key on a message's metadata map, creating the map if absent.
pub(crate) fn meta_set(metadata: &mut Option<ra2a::types::Metadata>, key: &str, value: Value) {
    metadata
        .get_or_insert_with(std::collections::HashMap::new)
        .insert(key.to_owned(), value);
}

/// Per-skill pricing entry. Server operators supply this (typically parsed
/// from the agent card's skill metadata) so the gate can price requests
/// without hardcoding amounts.
#[derive(Debug, Clone)]
pub struct SkillPricing {
    /// Serialized `PaymentRequirements` (wire camelCase) accepted for this skill.
    pub requirements: Vec<Value>,
    /// Optional human-readable description surfaced in the offer.
    pub description: Option<String>,
}

/// A settled-payment receipt, mirroring the spec's `x402SettleResponse`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Receipt {
    /// Whether settlement succeeded.
    pub success: bool,
    /// On-chain transaction hash (present only on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<String>,
    /// Network the payment settled on.
    pub network: String,
    /// Payer address, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payer: Option<String>,
    /// Error reason for unsuccessful settlement (spec error codes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_reason: Option<String>,
}
