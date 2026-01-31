//! Agent Card resolver for fetching and caching agent cards.

use reqwest::Client as HttpClient;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::{A2AError, Result};
use crate::types::AgentCard;

/// Well-known path for agent card discovery.
pub const AGENT_CARD_WELL_KNOWN_PATH: &str = ".well-known/agent.json";

/// Agent Card resolver for fetching agent cards from agents.
#[derive(Debug, Clone)]
pub struct A2ACardResolver {
    /// HTTP client for making requests.
    http_client: HttpClient,
    /// Base URL of the agent.
    base_url: String,
    /// Path to the agent card endpoint.
    agent_card_path: String,
    /// Cached agent card.
    cached_card: Arc<RwLock<Option<AgentCard>>>,
}

impl A2ACardResolver {
    /// Creates a new card resolver.
    pub fn new(http_client: HttpClient, base_url: impl Into<String>) -> Self {
        Self::with_path(http_client, base_url, AGENT_CARD_WELL_KNOWN_PATH)
    }

    /// Creates a new card resolver with a custom agent card path.
    pub fn with_path(
        http_client: HttpClient,
        base_url: impl Into<String>,
        agent_card_path: impl Into<String>,
    ) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let agent_card_path = agent_card_path.into().trim_start_matches('/').to_string();

        Self {
            http_client,
            base_url,
            agent_card_path,
            cached_card: Arc::new(RwLock::new(None)),
        }
    }

    /// Returns the full URL to the agent card.
    pub fn card_url(&self) -> String {
        format!("{}/{}", self.base_url, self.agent_card_path)
    }

    /// Fetches the agent card from the server.
    ///
    /// # Arguments
    /// * `relative_path` - Optional relative path to override the default card path
    /// * `signature_verifier` - Optional function to verify agent card signatures
    pub async fn get_agent_card<F>(
        &self,
        relative_path: Option<&str>,
        signature_verifier: Option<F>,
    ) -> Result<AgentCard>
    where
        F: FnOnce(&AgentCard) -> Result<()>,
    {
        let path = relative_path
            .map(|p| p.trim_start_matches('/').to_string())
            .unwrap_or_else(|| self.agent_card_path.clone());

        let target_url = format!("{}/{}", self.base_url, path);

        let response = self
            .http_client
            .get(&target_url)
            .send()
            .await
            .map_err(|e| A2AError::Connection(format!("Failed to fetch agent card: {}", e)))?;

        if !response.status().is_success() {
            return Err(A2AError::Connection(format!(
                "Failed to fetch agent card from {}: HTTP {}",
                target_url,
                response.status()
            )));
        }

        let agent_card: AgentCard = response.json().await.map_err(|e| {
            A2AError::Json(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse agent card JSON: {}", e),
            )))
        })?;

        // Verify signatures if a verifier is provided
        if let Some(verifier) = signature_verifier {
            verifier(&agent_card)?;
        }

        // Cache the card
        {
            let mut cache = self.cached_card.write().await;
            *cache = Some(agent_card.clone());
        }

        Ok(agent_card)
    }

    /// Returns the cached agent card if available.
    pub async fn get_cached_card(&self) -> Option<AgentCard> {
        let cache = self.cached_card.read().await;
        cache.clone()
    }

    /// Clears the cached agent card.
    pub async fn clear_cache(&self) {
        let mut cache = self.cached_card.write().await;
        *cache = None;
    }

    /// Fetches the agent card, using cache if available.
    pub async fn get_agent_card_cached(&self) -> Result<AgentCard> {
        // Check cache first
        if let Some(card) = self.get_cached_card().await {
            return Ok(card);
        }

        // Fetch and cache
        self.get_agent_card::<fn(&AgentCard) -> Result<()>>(None, None)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_url_generation() {
        let client = HttpClient::new();
        let resolver = A2ACardResolver::new(client, "https://agent.example.com");
        assert_eq!(
            resolver.card_url(),
            "https://agent.example.com/.well-known/agent.json"
        );
    }

    #[test]
    fn test_card_url_with_trailing_slash() {
        let client = HttpClient::new();
        let resolver = A2ACardResolver::new(client, "https://agent.example.com/");
        assert_eq!(
            resolver.card_url(),
            "https://agent.example.com/.well-known/agent.json"
        );
    }
}
