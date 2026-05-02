use crate::core::state::AppState;
use crate::core::target::{Source, Status, Target, TargetKind};
use chrono::Utc;
use serde_json::json;

/// A fixture item that can be forced to a failed state via `--fail`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FailKey {
    pub kind: TargetKind,
    pub name: String,
}

impl std::str::FromStr for FailKey {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let (kind_str, name) = s
            .split_once(':')
            .ok_or_else(|| format!("expected KIND:NAME, got '{s}'"))?;
        let kind = match kind_str {
            "model" => TargetKind::Model,
            "agent" => TargetKind::Agent,
            "tool" => TargetKind::Tool,
            "website" => TargetKind::Website,
            other => return Err(format!("unknown kind '{other}'")),
        };
        Ok(FailKey {
            kind,
            name: name.to_string(),
        })
    }
}

/// Populate `state` with a fixed set of mock targets.
///
/// Targets named in `fail_keys` are given a `Status::Failed` instead of `Status::Ok`.
pub fn populate(state: &AppState, fail_keys: &[FailKey]) {
    let fixtures = fixtures();
    for mut target in fixtures {
        let should_fail = fail_keys
            .iter()
            .any(|k| k.kind == target.kind && k.name == target.name);
        if should_fail {
            target.status = Status::Failed {
                error: "forced failure (--fail flag)".into(),
                checked_at: Utc::now(),
            };
        } else {
            target.status = Status::Ok {
                details: probe_details(&target),
                checked_at: Utc::now(),
            };
        }
        state.upsert(target);
    }
}

fn probe_details(target: &Target) -> serde_json::Value {
    match target.kind {
        TargetKind::Model => json!({
            "models": [target.name.clone()]
        }),
        TargetKind::Agent => json!({
            "name": target.name.clone(),
            "version": "1.0"
        }),
        TargetKind::Tool => json!({
            "tools": [{"name": "read_file"}, {"name": "write_file"}]
        }),
        TargetKind::Website => json!({
            "status_code": 200
        }),
    }
}

fn fixtures() -> Vec<Target> {
    vec![
        Target::new(
            "model-gpt-4",
            TargetKind::Model,
            "gpt-4",
            "http://mock-model-gpt4.default.svc:8000/v1"
                .parse()
                .unwrap(),
            Source::Discovered {
                namespace: "default".into(),
                service: "mock-model-gpt4".into(),
            },
        ),
        Target::new(
            "model-llama-3",
            TargetKind::Model,
            "llama-3",
            "http://mock-model-llama3.default.svc:8000/v1"
                .parse()
                .unwrap(),
            Source::Manual,
        ),
        Target::new(
            "model-mistral",
            TargetKind::Model,
            "mistral",
            "http://mock-model-mistral.ai.svc:8000/v1".parse().unwrap(),
            Source::Discovered {
                namespace: "ai".into(),
                service: "mock-model-mistral".into(),
            },
        ),
        Target::new(
            "agent-planner",
            TargetKind::Agent,
            "planner",
            "http://mock-agent-planner.default.svc:8080"
                .parse()
                .unwrap(),
            Source::Discovered {
                namespace: "default".into(),
                service: "mock-agent-planner".into(),
            },
        ),
        Target::new(
            "agent-coder",
            TargetKind::Agent,
            "coder",
            "http://mock-agent-coder.default.svc:8080".parse().unwrap(),
            Source::Manual,
        ),
        Target::new(
            "tool-filesystem",
            TargetKind::Tool,
            "filesystem",
            "http://mock-mcp-fs.default.svc:3000/mcp".parse().unwrap(),
            Source::Discovered {
                namespace: "default".into(),
                service: "mock-mcp-fs".into(),
            },
        ),
        Target::new(
            "tool-github",
            TargetKind::Tool,
            "github",
            "http://mock-mcp-github.default.svc:3000/mcp"
                .parse()
                .unwrap(),
            Source::Manual,
        ),
        Target::new(
            "tool-search",
            TargetKind::Tool,
            "search",
            "http://mock-mcp-search.ai.svc:3000/mcp".parse().unwrap(),
            Source::Discovered {
                namespace: "ai".into(),
                service: "mock-mcp-search".into(),
            },
        ),
        Target::new(
            "website-anthropic",
            TargetKind::Website,
            "anthropic.com",
            "https://anthropic.com".parse().unwrap(),
            Source::Manual,
        ),
        Target::new(
            "website-openai",
            TargetKind::Website,
            "openai.com",
            "https://openai.com".parse().unwrap(),
            Source::Manual,
        ),
        Target::new(
            "website-github",
            TargetKind::Website,
            "github.com",
            "https://github.com".parse().unwrap(),
            Source::Manual,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fail_key_parse_valid() {
        let k: FailKey = "model:gpt-4".parse().unwrap();
        assert_eq!(k.kind, TargetKind::Model);
        assert_eq!(k.name, "gpt-4");
    }

    #[test]
    fn fail_key_parse_invalid() {
        assert!("model".parse::<FailKey>().is_err());
        assert!("unknown:foo".parse::<FailKey>().is_err());
    }

    #[test]
    fn populate_all_ok() {
        let state = AppState::new();
        populate(&state, &[]);
        assert!(state.target_count() > 0);
        for t in state.all() {
            assert!(t.status.is_ok(), "expected ok status for {}", t.name);
        }
    }

    #[test]
    fn populate_with_fail() {
        let state = AppState::new();
        let fail_keys = vec!["model:gpt-4".parse::<FailKey>().unwrap()];
        populate(&state, &fail_keys);
        let gpt4 = state.get("model-gpt-4").unwrap();
        assert!(!gpt4.status.is_ok());
        let llama = state.get("model-llama-3").unwrap();
        assert!(llama.status.is_ok());
    }
}
