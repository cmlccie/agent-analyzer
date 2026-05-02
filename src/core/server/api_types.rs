use crate::core::target::Target;
use serde::{Deserialize, Serialize};

/// Full state snapshot returned by `GET /api/v1/state`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateResponse {
    pub targets: Vec<Target>,
}

/// Single target returned by `GET /api/v1/targets/:id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetResponse {
    pub target: Target,
}

/// Error body returned on 4xx/5xx responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::target::{Source, Status, TargetKind};

    #[test]
    fn state_response_serializes() {
        let t = Target {
            id: "x".into(),
            kind: TargetKind::Model,
            name: "x".into(),
            url: "http://localhost".parse().unwrap(),
            source: Source::Manual,
            status: Status::Unknown,
            metadata: Default::default(),
        };
        let resp = StateResponse { targets: vec![t] };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"model\""));
    }
}
