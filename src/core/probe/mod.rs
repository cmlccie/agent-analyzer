pub mod a2a;
pub mod mcp;
pub mod model;
pub mod website;

use crate::core::state::AppState;
use crate::core::target::{FailureKind, Status, Target, TargetKind};
use chrono::Utc;
use std::sync::Arc;
use tokio::time::{Duration, interval};

/// Trait implemented by each protocol prober.
#[allow(async_fn_in_trait)]
pub trait Prober {
    /// Probe a single target and return its new status.
    async fn probe(&self, target: &Target) -> Status;
}

/// Run a single probe pass over all targets in `state`, updating their status.
pub async fn run_once(state: &AppState, timeout: Duration) -> anyhow::Result<()> {
    let http = reqwest::Client::builder().timeout(timeout).build()?;

    let targets = state.all();
    let mut handles = Vec::with_capacity(targets.len());

    let state = Arc::new(state.clone());
    let http = Arc::new(http);

    for target in targets {
        let state = Arc::clone(&state);
        let http = Arc::clone(&http);
        handles.push(tokio::spawn(async move {
            let status = probe_target(&http, &target).await;
            state.record(&target.id, status);
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Start a background task that probes all targets on `interval_duration`.
pub fn spawn_scheduler(
    state: AppState,
    interval_duration: Duration,
    http_timeout: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(interval_duration);
        loop {
            ticker.tick().await;
            if let Err(e) = run_once(&state, http_timeout).await {
                tracing::warn!("probe pass failed: {e}");
            }
        }
    })
}

async fn probe_target(http: &reqwest::Client, target: &Target) -> Status {
    match target.kind {
        TargetKind::Model => model::ModelProber::new(http.clone()).probe(target).await,
        TargetKind::Agent => a2a::A2aProber::new(http.clone()).probe(target).await,
        TargetKind::Tool => mcp::McpProber::new(http.clone()).probe(target).await,
        TargetKind::Website => {
            website::WebsiteProber::new(http.clone())
                .probe(target)
                .await
        }
    }
}

pub fn failed_status(kind: FailureKind, error: impl std::fmt::Display) -> Status {
    Status::Failed {
        kind,
        error: error.to_string(),
        checked_at: Utc::now(),
    }
}

/// Failed status for a non-success HTTP response.
pub fn http_failed(status: reqwest::StatusCode) -> Status {
    failed_status(
        FailureKind::Http {
            code: status.as_u16(),
        },
        format!("HTTP {status}"),
    )
}

/// Failed status for a transport error, classified and naming the root cause.
///
/// reqwest's top-level message is just "error sending request for url (...)", which reads the
/// same whether a network policy dropped the packets (timeout), rejected them (connection
/// refused/reset) or DNS failed. The cause is further down the source chain, so the chain is
/// flattened here; the URL is omitted because the detail view already shows it.
pub fn request_failed(error: reqwest::Error) -> Status {
    let error = error.without_url();
    let chain: Vec<&(dyn std::error::Error + 'static)> =
        std::iter::successors(Some(&error as &(dyn std::error::Error + 'static)), |e| {
            e.source()
        })
        .collect();

    let mut parts: Vec<String> = Vec::new();
    for msg in chain.iter().map(|e| e.to_string()) {
        if !parts.iter().any(|p| p.contains(&msg)) {
            parts.push(msg);
        }
    }
    if error.is_timeout() && !parts.iter().any(|p| p.contains("timed out")) {
        parts.push("timed out".into());
    }

    failed_status(classify(&error, &chain, &parts), parts.join(": "))
}

fn classify(
    error: &reqwest::Error,
    chain: &[&(dyn std::error::Error + 'static)],
    messages: &[String],
) -> FailureKind {
    use std::io::ErrorKind;

    let io_kind = chain
        .iter()
        .find_map(|e| e.downcast_ref::<std::io::Error>())
        .map(std::io::Error::kind);
    let mentions = |needle: &str| {
        messages
            .iter()
            .any(|m| m.to_ascii_lowercase().contains(needle))
    };

    if error.is_timeout() || io_kind == Some(ErrorKind::TimedOut) {
        FailureKind::Timeout
    } else if matches!(
        io_kind,
        Some(
            ErrorKind::ConnectionRefused
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
        )
    ) {
        FailureKind::Refused
    } else if mentions("dns error") {
        FailureKind::Dns
    } else if mentions("certificate") || mentions("tls") || mentions("handshake") {
        FailureKind::Tls
    } else if error.is_decode() {
        FailureKind::Protocol
    } else {
        FailureKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_status_includes_message() {
        let s = failed_status(FailureKind::Timeout, "timeout after 5s");
        assert!(!s.is_ok());
        if let Status::Failed { error, .. } = s {
            assert!(error.contains("timeout"));
        }
    }

    #[tokio::test]
    async fn request_failed_names_root_cause() {
        let http = reqwest::Client::new();
        let err = http.get("http://127.0.0.1:1/").send().await.unwrap_err();
        let Status::Failed { kind, error, .. } = request_failed(err) else {
            panic!("expected failed status");
        };
        assert_eq!(kind, FailureKind::Refused);
        assert!(error.to_lowercase().contains("refused"), "got: {error}");
        assert!(!error.contains("127.0.0.1"), "got: {error}");
    }

    #[tokio::test]
    async fn request_failed_classifies_dns() {
        let http = reqwest::Client::new();
        let err = http
            .get("http://no-such-host.invalid/")
            .send()
            .await
            .unwrap_err();
        assert!(matches!(
            request_failed(err),
            Status::Failed {
                kind: FailureKind::Dns,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn request_failed_classifies_timeout() {
        // A listener that accepts but never answers stands in for a policy that drops traffic.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        let err = http
            .get(format!("http://{addr}/"))
            .send()
            .await
            .unwrap_err();
        assert!(matches!(
            request_failed(err),
            Status::Failed {
                kind: FailureKind::Timeout,
                ..
            }
        ));
        drop(listener);
    }

    #[tokio::test]
    async fn run_once_does_not_panic_on_empty_state() {
        let state = AppState::new();
        run_once(&state, Duration::from_millis(100)).await.unwrap();
    }
}
