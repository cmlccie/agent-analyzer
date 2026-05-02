use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use url::Url;

/// The kind of AI component a target represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    Model,
    Agent,
    Tool,
    Website,
}

impl std::fmt::Display for TargetKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetKind::Model => write!(f, "model"),
            TargetKind::Agent => write!(f, "agent"),
            TargetKind::Tool => write!(f, "tool"),
            TargetKind::Website => write!(f, "website"),
        }
    }
}

/// How the target was discovered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    Discovered { namespace: String, service: String },
    Manual,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Discovered { namespace, service } => {
                write!(f, "k8s:{namespace}/{service}")
            }
            Source::Manual => write!(f, "manual"),
        }
    }
}

/// Current reachability status of a target.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Status {
    Unknown,
    Ok {
        details: serde_json::Value,
        checked_at: DateTime<Utc>,
    },
    Failed {
        error: String,
        checked_at: DateTime<Utc>,
    },
}

impl Status {
    pub fn is_ok(&self) -> bool {
        matches!(self, Status::Ok { .. })
    }

    pub fn checked_at(&self) -> Option<DateTime<Utc>> {
        match self {
            Status::Ok { checked_at, .. } | Status::Failed { checked_at, .. } => Some(*checked_at),
            Status::Unknown => None,
        }
    }
}

/// A single discoverable/configurable component to probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub kind: TargetKind,
    pub name: String,
    pub url: Url,
    pub source: Source,
    pub status: Status,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl Target {
    pub fn new(
        id: impl Into<String>,
        kind: TargetKind,
        name: impl Into<String>,
        url: Url,
        source: Source,
    ) -> Self {
        Target {
            id: id.into(),
            kind,
            name: name.into(),
            url,
            source,
            status: Status::Unknown,
            metadata: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_target(kind: TargetKind) -> Target {
        Target::new(
            "test-id",
            kind,
            "test",
            "http://localhost:8080".parse().unwrap(),
            Source::Manual,
        )
    }

    #[test]
    fn status_is_ok() {
        let mut t = make_target(TargetKind::Model);
        assert!(!t.status.is_ok());
        t.status = Status::Ok {
            details: serde_json::Value::Null,
            checked_at: Utc::now(),
        };
        assert!(t.status.is_ok());
    }

    #[test]
    fn source_display() {
        let s = Source::Discovered {
            namespace: "default".into(),
            service: "vllm".into(),
        };
        assert_eq!(s.to_string(), "k8s:default/vllm");
        assert_eq!(Source::Manual.to_string(), "manual");
    }

    #[test]
    fn target_kind_display() {
        assert_eq!(TargetKind::Model.to_string(), "model");
        assert_eq!(TargetKind::Website.to_string(), "website");
    }

    #[test]
    fn target_serializes() {
        let t = make_target(TargetKind::Tool);
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"tool\""));
    }
}
