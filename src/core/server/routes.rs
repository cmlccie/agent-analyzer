use crate::core::server::api_types::{ErrorResponse, StateResponse, TargetResponse};
use crate::core::state::AppState;
use crate::core::target::TargetKind;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct KindFilter {
    pub kind: Option<TargetKind>,
}

pub async fn healthz() -> impl IntoResponse {
    StatusCode::OK
}

pub async fn get_state(State(state): State<AppState>) -> impl IntoResponse {
    let targets = state.all();
    Json(StateResponse { targets })
}

pub async fn get_targets(
    State(state): State<AppState>,
    Query(filter): Query<KindFilter>,
) -> impl IntoResponse {
    let targets = match filter.kind {
        Some(kind) => state.by_kind(kind),
        None => state.all(),
    };
    Json(StateResponse { targets })
}

pub async fn get_target(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.get(&id) {
        Some(target) => Json(TargetResponse { target }).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("target '{id}' not found"),
            }),
        )
            .into_response(),
    }
}
