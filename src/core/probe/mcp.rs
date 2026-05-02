//! MCP server probe — performs an initialize + tools/list handshake.
//!
//! Uses `rmcp` (Rust MCP SDK). In this skeleton the prober does a lightweight
//! HTTP probe on the MCP endpoint; the full rmcp integration is wired in step 6.

use crate::core::probe::{Prober, failed_status};
use crate::core::target::{Status, Target};
use chrono::Utc;
use reqwest::Client;
use serde_json::json;

pub struct McpProber {
    http: Client,
}

impl McpProber {
    pub fn new(http: Client) -> Self {
        McpProber { http }
    }
}

impl Prober for McpProber {
    async fn probe(&self, target: &Target) -> Status {
        // Step 6 will replace this with a full rmcp initialize handshake.
        // For now we verify the endpoint responds to a POST with a valid
        // JSON-RPC initialize request.
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "agent-analyzer",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        match self.http.post(target.url.as_str()).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                let details: serde_json::Value = resp.json().await.unwrap_or(json!({}));
                Status::Ok {
                    details,
                    checked_at: Utc::now(),
                }
            }
            Ok(resp) => failed_status(format!("HTTP {}", resp.status())),
            Err(e) => failed_status(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::target::{Source, TargetKind};

    #[tokio::test]
    async fn unreachable_mcp_returns_failed() {
        let http = Client::builder()
            .timeout(std::time::Duration::from_millis(100))
            .build()
            .unwrap();
        let prober = McpProber::new(http);
        let target = crate::core::target::Target::new(
            "mcp-test",
            TargetKind::Tool,
            "test",
            "http://127.0.0.1:1/mcp".parse().unwrap(),
            Source::Manual,
        );
        let status = prober.probe(&target).await;
        assert!(!status.is_ok());
    }
}
