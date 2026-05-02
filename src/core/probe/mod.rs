pub mod a2a;
pub mod mcp;
pub mod model;
pub mod website;

use crate::core::state::AppState;
use crate::core::target::{Status, Target, TargetKind};
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
            let mut updated = target.clone();
            updated.status = status;
            state.upsert(updated);
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
        ticker.tick().await; // skip the immediate first tick
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

pub fn failed_status(error: impl std::fmt::Display) -> Status {
    Status::Failed {
        error: error.to_string(),
        checked_at: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_status_includes_message() {
        let s = failed_status("timeout after 5s");
        assert!(!s.is_ok());
        if let Status::Failed { error, .. } = s {
            assert!(error.contains("timeout"));
        }
    }

    #[tokio::test]
    async fn run_once_does_not_panic_on_empty_state() {
        let state = AppState::new();
        run_once(&state, Duration::from_millis(100)).await.unwrap();
    }
}
