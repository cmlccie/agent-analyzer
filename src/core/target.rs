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

/// Why a probe failed, classified so a network-policy or auth outcome can be read at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FailureKind {
    /// No response before the timeout — typically a policy silently dropping packets.
    Timeout,
    /// Connection refused or reset — a policy rejecting traffic, or nothing listening.
    Refused,
    /// The name did not resolve.
    Dns,
    /// TLS handshake or certificate failure.
    Tls,
    /// The endpoint answered with a non-success HTTP status.
    Http { code: u16 },
    /// The endpoint answered, but not with the expected protocol response.
    Protocol,
    #[default]
    Other,
}

impl FailureKind {
    /// Short label for list rows and summaries.
    pub fn label(&self) -> String {
        match self {
            FailureKind::Timeout => "timeout".into(),
            FailureKind::Refused => "refused".into(),
            FailureKind::Dns => "dns".into(),
            FailureKind::Tls => "tls".into(),
            FailureKind::Http { code: 401 } => "unauthorized".into(),
            FailureKind::Http { code: 403 } => "forbidden".into(),
            FailureKind::Http { code } => format!("http {code}"),
            FailureKind::Protocol => "bad response".into(),
            FailureKind::Other => "error".into(),
        }
    }

    /// What the failure most likely means, for the detail view.
    pub fn explanation(&self) -> &'static str {
        match self {
            FailureKind::Timeout => "no response; traffic likely dropped by policy",
            FailureKind::Refused => "connection rejected, or nothing listening",
            FailureKind::Dns => "name did not resolve",
            FailureKind::Tls => "TLS handshake failed",
            FailureKind::Http { code: 401 } => "reachable, but authentication required",
            FailureKind::Http { code: 403 } => "reachable, but access denied",
            FailureKind::Http { .. } => "reachable, but returned an HTTP error",
            FailureKind::Protocol => "reachable, but not speaking the expected protocol",
            FailureKind::Other => "request failed",
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
        #[serde(default)]
        kind: FailureKind,
        error: String,
        checked_at: DateTime<Utc>,
    },
}

impl Status {
    pub fn is_ok(&self) -> bool {
        matches!(self, Status::Ok { .. })
    }

    /// Whether two statuses describe the same state (ignoring details, error text and time), so a
    /// run of identical results is one continuous state.
    fn same_state(&self, other: &Status) -> bool {
        match (self, other) {
            (Status::Unknown, Status::Unknown) | (Status::Ok { .. }, Status::Ok { .. }) => true,
            (Status::Failed { kind: a, .. }, Status::Failed { kind: b, .. }) => a == b,
            _ => false,
        }
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
    /// When the target entered its current state (reachable, or failing for the same reason).
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
    /// Recent probe outcomes, oldest first; `true` means reachable.
    #[serde(default)]
    pub history: Vec<bool>,
}

/// Number of probe outcomes kept in `Target::history`.
pub const HISTORY_LEN: usize = 30;

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
            since: None,
            history: Vec::new(),
        }
    }

    /// Record a probe result: update the status, restart `since` when the state changes, and
    /// append to the bounded history.
    pub fn record(&mut self, status: Status) {
        if self.since.is_none() || !self.status.same_state(&status) {
            self.since = status.checked_at();
        }
        self.history.push(status.is_ok());
        let excess = self.history.len().saturating_sub(HISTORY_LEN);
        self.history.drain(..excess);
        self.status = status;
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

    fn failed(kind: FailureKind, at: DateTime<Utc>) -> Status {
        Status::Failed {
            kind,
            error: "x".into(),
            checked_at: at,
        }
    }

    #[test]
    fn record_keeps_since_while_state_is_unchanged() {
        let mut t = make_target(TargetKind::Agent);
        let t0 = Utc::now();
        let t1 = t0 + chrono::Duration::seconds(10);
        t.record(failed(FailureKind::Timeout, t0));
        t.record(failed(FailureKind::Timeout, t1));
        assert_eq!(t.since, Some(t0));

        t.record(failed(FailureKind::Refused, t1));
        assert_eq!(t.since, Some(t1), "a different failure kind is a new state");
        assert_eq!(t.history, [false, false, false]);
    }

    #[test]
    fn record_bounds_history() {
        let mut t = make_target(TargetKind::Model);
        for _ in 0..HISTORY_LEN + 5 {
            t.record(failed(FailureKind::Dns, Utc::now()));
        }
        t.record(Status::Ok {
            details: serde_json::Value::Null,
            checked_at: Utc::now(),
        });
        assert_eq!(t.history.len(), HISTORY_LEN);
        assert_eq!(t.history.last(), Some(&true));
    }

    #[test]
    fn failure_kind_labels() {
        assert_eq!(FailureKind::Http { code: 401 }.label(), "unauthorized");
        assert_eq!(FailureKind::Http { code: 502 }.label(), "http 502");
    }
}
