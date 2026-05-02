use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use url::Url;

/// Top-level configuration loaded from `config.yaml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub kubernetes: KubernetesConfig,
    pub manual: ManualConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// How often to re-run Kubernetes discovery (seconds).
    pub discovery_interval_secs: u64,
    /// How often to re-probe each target (seconds).
    pub probe_interval_secs: u64,
    /// Per-request HTTP timeout (seconds).
    pub http_timeout_secs: u64,
}

impl GeneralConfig {
    pub fn discovery_interval(&self) -> Duration {
        Duration::from_secs(self.discovery_interval_secs)
    }

    pub fn probe_interval(&self) -> Duration {
        Duration::from_secs(self.probe_interval_secs)
    }

    pub fn http_timeout(&self) -> Duration {
        Duration::from_secs(self.http_timeout_secs)
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        GeneralConfig {
            discovery_interval_secs: 30,
            probe_interval_secs: 10,
            http_timeout_secs: 5,
        }
    }
}

/// Kubernetes discovery configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KubernetesConfig {
    pub enabled: bool,
    /// Namespaces to watch; empty means all accessible namespaces.
    pub namespaces: Vec<String>,
    pub label_selectors: LabelSelectors,
}

/// Standard Kubernetes label-selector strings (equality, set-based, comma-AND).
/// Operators supply whatever labels their existing services carry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LabelSelectors {
    pub models: Option<String>,
    pub agents: Option<String>,
    pub tools: Option<String>,
}

/// Manually configured static targets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ManualConfig {
    pub websites: Vec<ManualTarget>,
    pub models: Vec<ManualTarget>,
    pub agents: Vec<ManualTarget>,
    pub tools: Vec<ManualTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualTarget {
    pub name: String,
    pub url: Url,
}

/// Load a [`Config`] from a YAML file on disk.
pub fn load(path: &Path) -> anyhow::Result<Config> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    parse(&content)
}

/// Parse a [`Config`] from a YAML string.
pub fn parse(yaml: &str) -> anyhow::Result<Config> {
    serde_yaml_neo::from_str(yaml).map_err(|e| anyhow::anyhow!("invalid YAML: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
general:
  discovery_interval_secs: 60
  probe_interval_secs: 15
  http_timeout_secs: 3

kubernetes:
  enabled: true
  namespaces: [default, ai]
  label_selectors:
    models: "app=vllm"
    agents: "app=a2a-agent"
    tools: "app in (mcp-fs, mcp-github)"

manual:
  websites:
    - name: anthropic
      url: https://anthropic.com
  models: []
  agents: []
  tools: []
"#;

    #[test]
    fn parses_sample_config() {
        let cfg = parse(SAMPLE).expect("sample config should parse");
        assert_eq!(cfg.general.discovery_interval_secs, 60);
        assert!(cfg.kubernetes.enabled);
        assert_eq!(cfg.kubernetes.namespaces, vec!["default", "ai"]);
        assert_eq!(
            cfg.kubernetes.label_selectors.models.as_deref(),
            Some("app=vllm")
        );
        assert_eq!(cfg.manual.websites.len(), 1);
        assert_eq!(cfg.manual.websites[0].name, "anthropic");
    }

    #[test]
    fn defaults_are_sensible() {
        let cfg = Config::default();
        assert_eq!(cfg.general.probe_interval_secs, 10);
        assert!(!cfg.kubernetes.enabled);
        assert!(cfg.manual.websites.is_empty());
    }

    #[test]
    fn empty_yaml_gives_defaults() {
        let cfg = parse("{}").unwrap();
        assert_eq!(cfg.general.http_timeout_secs, 5);
    }

    #[test]
    fn invalid_yaml_errors() {
        assert!(parse("{{invalid").is_err());
    }
}
