//! MCP server probe — performs the Streamable HTTP handshake and lists the server's tools.
//!
//! Sequence: `initialize` → `notifications/initialized` → `tools/list`, then the session is
//! closed with `DELETE` so frequent probing does not accumulate sessions on the server. Responses
//! may be plain JSON or a `text/event-stream`, as the transport allows either.
//!
//! The target counts as reachable once `initialize` succeeds; a failing `tools/list` (e.g. an
//! authorization policy that admits the handshake but not tool access) is reported in the details
//! rather than as a failure.

use crate::core::probe::{Prober, failed_status, http_failed, request_failed};
use crate::core::target::{FailureKind, Status, Target};
use chrono::Utc;
use reqwest::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde_json::{Value, json};

const PROTOCOL_VERSION: &str = "2025-06-18";
const SESSION_HEADER: &str = "mcp-session-id";
const VERSION_HEADER: &str = "mcp-protocol-version";

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
        match self.inventory(target.url.as_str()).await {
            Ok(details) => Status::Ok {
                details,
                checked_at: Utc::now(),
            },
            Err(status) => status,
        }
    }
}

/// An open MCP session: the negotiated protocol version and the server-assigned session id.
struct Session {
    version: String,
    id: Option<String>,
}

impl McpProber {
    async fn inventory(&self, url: &str) -> Result<Value, Status> {
        let initialize = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "agent-analyzer",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        let resp = self.post(url, None, &initialize).await?;
        let session_id = resp
            .headers()
            .get(SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let init = rpc_result(resp, 1).await?;

        let session = Session {
            version: init["protocolVersion"]
                .as_str()
                .unwrap_or(PROTOCOL_VERSION)
                .to_string(),
            id: session_id,
        };

        let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        let _ = self.post(url, Some(&session), &initialized).await;

        let tools_list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"});
        let tools = match self.post(url, Some(&session), &tools_list).await {
            Ok(resp) => rpc_result(resp, 2).await,
            Err(status) => Err(status),
        };

        self.close(url, &session).await;

        let server_info = &init["serverInfo"];
        let server = [
            server_info["name"].as_str(),
            server_info["version"].as_str(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        let mut details = json!({ "server": server, "protocol": session.version });
        match tools {
            Ok(result) => details["tools"] = json!(names(&result["tools"])),
            Err(Status::Failed { error, .. }) => details["tools_error"] = json!(error),
            Err(_) => {}
        }
        Ok(details)
    }

    async fn post(
        &self,
        url: &str,
        session: Option<&Session>,
        body: &Value,
    ) -> Result<reqwest::Response, Status> {
        let mut req = self
            .http
            .post(url)
            .header(ACCEPT, "application/json, text/event-stream")
            .json(body);
        if let Some(s) = session {
            req = req.header(VERSION_HEADER, &s.version);
            if let Some(id) = &s.id {
                req = req.header(SESSION_HEADER, id);
            }
        }
        let resp = req.send().await.map_err(request_failed)?;
        if resp.status().is_success() {
            Ok(resp)
        } else {
            Err(http_failed(resp.status()))
        }
    }

    async fn close(&self, url: &str, session: &Session) {
        if let Some(id) = &session.id {
            let _ = self
                .http
                .delete(url)
                .header(SESSION_HEADER, id)
                .header(VERSION_HEADER, &session.version)
                .send()
                .await;
        }
    }
}

/// Extract the JSON-RPC `result` for request `id` from a JSON or SSE response.
async fn rpc_result(resp: reqwest::Response, id: u64) -> Result<Value, Status> {
    let is_sse = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/event-stream"));
    let body = resp.text().await.map_err(request_failed)?;

    let messages: Vec<Value> = if is_sse {
        sse_data(&body)
            .filter_map(|data| serde_json::from_str(&data).ok())
            .collect()
    } else {
        serde_json::from_str(&body).into_iter().collect()
    };

    let message = messages
        .into_iter()
        .find(|m| m["id"] == json!(id))
        .ok_or_else(|| failed_status(FailureKind::Protocol, "no JSON-RPC response from server"))?;

    match (message.get("result"), message.get("error")) {
        (Some(result), _) => Ok(result.clone()),
        (None, Some(error)) => Err(failed_status(
            FailureKind::Protocol,
            format!(
                "JSON-RPC error: {}",
                error["message"].as_str().unwrap_or("unknown")
            ),
        )),
        (None, None) => Err(failed_status(
            FailureKind::Protocol,
            "JSON-RPC response has no result",
        )),
    }
}

/// The `data` payload of each event in a Server-Sent Events body.
fn sse_data(body: &str) -> impl Iterator<Item = String> + '_ {
    body.split("\n\n").filter_map(|event| {
        let data: Vec<&str> = event
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim_start)
            .collect();
        (!data.is_empty()).then(|| data.join("\n"))
    })
}

/// The `name` of each object in a JSON array.
pub(crate) fn names(items: &Value) -> Vec<String> {
    items
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| i["name"].as_str().map(String::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::target::{Source, TargetKind};
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::{Json, Router, routing::post};

    fn target(url: &str) -> Target {
        Target::new(
            "mcp-test",
            TargetKind::Tool,
            "test",
            url.parse().unwrap(),
            Source::Manual,
        )
    }

    fn prober() -> McpProber {
        McpProber::new(
            Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .build()
                .unwrap(),
        )
    }

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/mcp")
    }

    /// A minimal Streamable HTTP MCP server that answers over SSE and requires its session id.
    async fn mcp_handler(headers: HeaderMap, Json(req): Json<Value>) -> axum::response::Response {
        let reply = |result: Value| {
            let msg = json!({"jsonrpc": "2.0", "id": req["id"], "result": result});
            (
                [
                    (CONTENT_TYPE, "text/event-stream"),
                    (SESSION_HEADER.parse().unwrap(), "s1"),
                ],
                format!("event: message\ndata: {msg}\n\n"),
            )
                .into_response()
        };
        match req["method"].as_str() {
            Some("initialize") => reply(json!({
                "protocolVersion": "2025-03-26",
                "serverInfo": {"name": "weather", "version": "1.2.0"},
                "capabilities": {"tools": {}}
            })),
            Some("notifications/initialized") => StatusCode::ACCEPTED.into_response(),
            Some("tools/list") if headers.get(SESSION_HEADER).is_some_and(|v| v == "s1") => {
                reply(json!({"tools": [{"name": "get_forecast"}, {"name": "get_alerts"}]}))
            }
            _ => StatusCode::BAD_REQUEST.into_response(),
        }
    }

    #[tokio::test]
    async fn lists_tools_over_sse_session() {
        let url =
            serve(Router::new().route("/mcp", post(mcp_handler).delete(|| async { "" }))).await;
        let Status::Ok { details, .. } = prober().probe(&target(&url)).await else {
            panic!("expected ok");
        };
        assert_eq!(details["server"], "weather 1.2.0");
        assert_eq!(details["protocol"], "2025-03-26");
        assert_eq!(details["tools"], json!(["get_forecast", "get_alerts"]));
    }

    #[tokio::test]
    async fn http_error_is_classified() {
        let url =
            serve(Router::new().route("/mcp", post(|| async { StatusCode::UNAUTHORIZED }))).await;
        let status = prober().probe(&target(&url)).await;
        assert!(matches!(
            status,
            Status::Failed {
                kind: FailureKind::Http { code: 401 },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn non_mcp_endpoint_is_a_protocol_failure() {
        let url = serve(Router::new().route("/mcp", post(|| async { "hello" }))).await;
        let status = prober().probe(&target(&url)).await;
        assert!(matches!(
            status,
            Status::Failed {
                kind: FailureKind::Protocol,
                ..
            }
        ));
    }

    #[test]
    fn sse_data_joins_multiline_events() {
        let body = "event: message\ndata: {\"a\":\ndata: 1}\n\ndata: {\"b\":2}\n\n";
        let events: Vec<String> = sse_data(body).collect();
        assert_eq!(events, ["{\"a\":\n1}", "{\"b\":2}"]);
    }

    #[tokio::test]
    async fn unreachable_mcp_returns_failed() {
        let status = prober().probe(&target("http://127.0.0.1:1/mcp")).await;
        assert!(!status.is_ok());
    }
}
