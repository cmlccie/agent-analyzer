//! A2A agent probe — fetches the agent card at `/.well-known/agent.json`.
//!
//! Implements a thin HTTP layer directly against the A2A spec (no external
//! A2A library dependency; `fasa2a` does not exist on crates.io).

use crate::core::probe::{Prober, failed_status};
use crate::core::target::{Status, Target};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Partial A2A Agent Card as defined in the A2A specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

pub struct A2aProber {
    http: Client,
}

impl A2aProber {
    pub fn new(http: Client) -> Self {
        A2aProber { http }
    }
}

impl Prober for A2aProber {
    async fn probe(&self, target: &Target) -> Status {
        let card_url = agent_card_url(&target.url);

        match self.http.get(&card_url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.json::<AgentCard>().await {
                Ok(card) => {
                    let details = serde_json::to_value(&card).unwrap_or(serde_json::Value::Null);
                    Status::Ok {
                        details,
                        checked_at: Utc::now(),
                    }
                }
                Err(e) => failed_status(format!("invalid agent card: {e}")),
            },
            Ok(resp) => failed_status(format!("HTTP {}", resp.status())),
            Err(e) => failed_status(e),
        }
    }
}

fn agent_card_url(base: &url::Url) -> String {
    let mut u = base.clone();
    u.set_path("/.well-known/agent.json");
    u.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_card_url_replaces_path() {
        let base: url::Url = "http://localhost:8080/some/path".parse().unwrap();
        let url = agent_card_url(&base);
        assert_eq!(url, "http://localhost:8080/.well-known/agent.json");
    }

    #[test]
    fn agent_card_deserializes() {
        let json = r#"{"name":"planner","description":"planning agent","version":"1.0"}"#;
        let card: AgentCard = serde_json::from_str(json).unwrap();
        assert_eq!(card.name, "planner");
    }

    #[tokio::test]
    async fn unreachable_agent_returns_failed() {
        let http = Client::builder()
            .timeout(std::time::Duration::from_millis(100))
            .build()
            .unwrap();
        let prober = A2aProber::new(http);
        let target = crate::core::target::Target::new(
            "a2a-test",
            crate::core::target::TargetKind::Agent,
            "test",
            "http://127.0.0.1:1".parse().unwrap(),
            crate::core::target::Source::Manual,
        );
        let status = prober.probe(&target).await;
        assert!(!status.is_ok());
    }
}
