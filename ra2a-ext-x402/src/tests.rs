//! Integration-style tests for the x402 extension (spec v0.1 handshake).

#[cfg(test)]
mod x402_handshake {

    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::{
        KEY_PAYMENT_REQUIRED, KEY_PAYMENT_STATUS, KEY_SKILL_ID, MapPricing, PaymentGate,
        PaymentStatus, SkillPricing, X402_EXTENSION_URI, X402Extension, submitted_message,
        submitted_task_id,
    };
    use ra2a::server::CallInterceptor as _;
    use ra2a::types::{
        AgentCapabilities, AgentCard, AgentExtension, AgentInterface, Message, SendMessageRequest,
        TransportProtocol,
    };

    fn plain_card() -> AgentCard {
        let mut card = AgentCard::new(
            "test",
            "test agent",
            vec![AgentInterface::new(
                "https://example.com",
                TransportProtocol::new("JSONRPC"),
            )],
        );
        card.capabilities = AgentCapabilities::default();
        card
    }

    fn sample_pricing() -> HashMap<String, SkillPricing> {
        let mut m = HashMap::new();
        m.insert(
            String::from("generate-image"),
            SkillPricing {
                requirements: vec![serde_json::json!({
                    "scheme": "exact",
                    "network": "eip155:8453",
                    "amount": "10000",
                    "asset": "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
                    "extra": { "name": "USD Coin", "version": "2" }
                })],
                description: Some(String::from("Generate an image")),
            },
        );
        m
    }

    fn activated_request(message: Message) -> (ra2a::server::CallContext, ra2a::server::Request) {
        let mut ctx = ra2a::server::CallContext::new(
            "message/send",
            ra2a::server::RequestMeta::new(HashMap::new()),
        );
        ctx.activate_extension(X402_EXTENSION_URI);
        let req = ra2a::server::Request::new(SendMessageRequest::new(message));
        (ctx, req)
    }

    #[tokio::test]
    async fn gate_parks_unpaid_request_as_input_required() {
        let gate = PaymentGate::new(
            Arc::new(MapPricing(sample_pricing())),
            "0xmerchant",
            "https://agent.example",
        );
        let mut msg = Message::user_text("make me an image");
        msg.metadata.get_or_insert_with(HashMap::new).insert(
            String::from(KEY_SKILL_ID),
            serde_json::json!("generate-image"),
        );

        let (mut ctx, mut req) = activated_request(msg);
        let err = gate.before(&mut ctx, &mut req).await.unwrap_err();

        // The short-circuit carries the parked task.
        #[allow(clippy::panic, reason = "test assertion failure path")]
        let body = match &err {
            ra2a::error::A2AError::ServerError(s) => s.clone(),
            other => panic!("expected ServerError, got {other:?}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let p = |path: &str| parsed.pointer(path);
        assert_eq!(
            p("/status").and_then(|v| v.as_str()),
            Some(PaymentStatus::Required.as_str())
        );
        assert_eq!(
            p("/task/status/state").and_then(|v| v.as_str()),
            Some("TASK_STATE_INPUT_REQUIRED")
        );

        let status = p("/task/status/message/metadata")
            .and_then(|m| m.get(KEY_PAYMENT_STATUS))
            .and_then(|v| v.as_str());
        assert_eq!(status, Some(PaymentStatus::Required.as_str()));

        let accepts = p("/task/status/message/metadata")
            .and_then(|m| m.get(KEY_PAYMENT_REQUIRED))
            .and_then(|o| o.get("accepts"))
            .and_then(|a| a.as_array())
            .and_then(|a| a.first());
        assert_eq!(
            accepts
                .and_then(|o| o.get("scheme"))
                .and_then(|v| v.as_str()),
            Some("exact")
        );
        assert_eq!(
            accepts
                .and_then(|o| o.get("payTo"))
                .and_then(|v| v.as_str()),
            Some("0xmerchant")
        );
        assert_eq!(
            accepts
                .and_then(|o| o.get("resource"))
                .and_then(|v| v.as_str()),
            Some("https://agent.example/skills/generate-image")
        );
        let _ = p;
    }

    #[tokio::test]
    async fn gate_passes_through_when_extension_not_activated() {
        let gate = PaymentGate::new(
            Arc::new(MapPricing(sample_pricing())),
            "0xmerchant",
            "https://agent.example",
        );
        let mut msg = Message::user_text("make me an image");
        msg.metadata.get_or_insert_with(HashMap::new).insert(
            String::from(KEY_SKILL_ID),
            serde_json::json!("generate-image"),
        );

        // No activation on this context.
        let mut ctx = ra2a::server::CallContext::new(
            "message/send",
            ra2a::server::RequestMeta::new(HashMap::new()),
        );
        let mut req = ra2a::server::Request::new(SendMessageRequest::new(msg));

        gate.before(&mut ctx, &mut req).await.unwrap(); // no error → passed through
    }

    #[tokio::test]
    async fn gate_passes_through_free_skills() {
        let gate = PaymentGate::new(
            Arc::new(MapPricing(HashMap::new())),
            "0xmerchant",
            "https://agent.example",
        );
        let mut msg = Message::user_text("hello");
        msg.metadata
            .get_or_insert_with(HashMap::new)
            .insert(String::from(KEY_SKILL_ID), serde_json::json!("free-skill"));

        let (mut ctx, mut req) = activated_request(msg);
        gate.before(&mut ctx, &mut req).await.unwrap();
    }

    #[tokio::test]
    async fn gate_passes_through_payment_submitted() {
        let gate = PaymentGate::new(
            Arc::new(MapPricing(sample_pricing())),
            "0xmerchant",
            "https://agent.example",
        );
        let mut msg = Message::user_text("paying up");
        let meta = msg.metadata.get_or_insert_with(HashMap::new);
        meta.insert(
            String::from(KEY_SKILL_ID),
            serde_json::json!("generate-image"),
        );
        meta.insert(
            String::from(KEY_PAYMENT_STATUS),
            serde_json::json!(PaymentStatus::Submitted.as_str()),
        );

        let (mut ctx, mut req) = activated_request(msg);
        gate.before(&mut ctx, &mut req).await.unwrap();
    }

    #[test]
    fn submitted_message_carries_task_id_and_payload() {
        let msg = submitted_message(
            "task-123",
            serde_json::json!({
                "x402Version": 2,
                "scheme": "exact",
                "network": "eip155:8453",
                "payload": { "signature": "0xdeadbeef" }
            }),
        );
        assert_eq!(
            submitted_task_id(&msg).map(ra2a::types::TaskId::as_str),
            Some("task-123")
        );
        let meta = msg.metadata.as_ref().unwrap();
        assert_eq!(
            meta.get(KEY_PAYMENT_STATUS).and_then(|v| v.as_str()),
            Some(PaymentStatus::Submitted.as_str())
        );
        assert!(meta.get(crate::KEY_PAYMENT_PAYLOAD).is_some());
    }

    #[test]
    fn declare_on_card_is_idempotent() {
        let mut card = plain_card();
        X402Extension::declare_on_card(&mut card);
        let count = card.capabilities.extensions.len();
        X402Extension::declare_on_card(&mut card);
        assert_eq!(card.capabilities.extensions.len(), count);
        assert!(
            card.capabilities
                .extensions
                .iter()
                .any(|e| e.uri == X402_EXTENSION_URI)
        );
    }

    #[test]
    fn declare_on_card_sets_required_true() {
        let mut card = plain_card();
        X402Extension::declare_on_card(&mut card);
        let ext: Option<&AgentExtension> = card
            .capabilities
            .extensions
            .iter()
            .find(|e| e.uri == X402_EXTENSION_URI);
        assert_eq!(ext.map(|e| e.required), Some(true));
    }

    #[test]
    fn payment_status_round_trips() {
        for (wire, status) in [
            ("payment-required", PaymentStatus::Required),
            ("payment-submitted", PaymentStatus::Submitted),
            ("payment-rejected", PaymentStatus::Rejected),
            ("payment-verified", PaymentStatus::Verified),
            ("payment-completed", PaymentStatus::Completed),
            ("payment-failed", PaymentStatus::Failed),
        ] {
            assert_eq!(PaymentStatus::from_wire(wire), Some(status));
            assert_eq!(status.as_str(), wire);
        }
        assert_eq!(PaymentStatus::from_wire("bogus"), None);
    }
}
