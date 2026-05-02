pub mod api_types;
pub mod routes;

use crate::core::state::AppState;
use anyhow::Context;
use axum::{Router, routing::get};
use std::net::SocketAddr;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/healthz", get(routes::healthz))
        .route("/api/v1/state", get(routes::get_state))
        .route("/api/v1/targets", get(routes::get_targets))
        .route("/api/v1/targets/{id}", get(routes::get_target))
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: AppState) -> anyhow::Result<()> {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    tracing::info!("server listening on {addr}");
    axum::serve(listener, app).await.context("server error")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::target::{Source, Status, Target, TargetKind};
    use axum::http::StatusCode;
    use axum_test::TestServer;

    fn make_target(id: &str, kind: TargetKind) -> Target {
        Target {
            id: id.to_string(),
            kind,
            name: id.to_string(),
            url: "http://localhost".parse().unwrap(),
            source: Source::Manual,
            status: Status::Unknown,
            metadata: Default::default(),
        }
    }

    #[tokio::test]
    async fn healthz_returns_200() {
        let state = AppState::new();
        let server = TestServer::new(router(state));
        let resp = server.get("/api/v1/healthz").await;
        resp.assert_status(StatusCode::OK);
    }

    #[tokio::test]
    async fn state_returns_all_targets() {
        let state = AppState::new();
        state.upsert(make_target("m1", TargetKind::Model));
        let server = TestServer::new(router(state));
        let resp = server.get("/api/v1/state").await;
        resp.assert_status_ok();
        let body: serde_json::Value = resp.json();
        assert_eq!(body["targets"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn target_not_found_returns_404() {
        let state = AppState::new();
        let server = TestServer::new(router(state));
        let resp = server.get("/api/v1/targets/nonexistent").await;
        resp.assert_status(StatusCode::NOT_FOUND);
    }
}
