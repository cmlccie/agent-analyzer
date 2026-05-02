use crate::core::config::Config;
use crate::core::discovery::Discoverer;
use crate::core::state::AppState;
use crate::core::target::{Source, Target, TargetKind};

pub struct ManualDiscoverer {
    config: Config,
}

impl ManualDiscoverer {
    pub fn new(config: &Config) -> Self {
        ManualDiscoverer {
            config: config.clone(),
        }
    }
}

impl Discoverer for ManualDiscoverer {
    async fn discover(&self, state: &AppState) -> anyhow::Result<()> {
        let m = &self.config.manual;
        for (kind, entries) in [
            (TargetKind::Model, &m.models),
            (TargetKind::Agent, &m.agents),
            (TargetKind::Tool, &m.tools),
            (TargetKind::Website, &m.websites),
        ] {
            for entry in entries {
                let id = format!("manual-{kind}-{}", entry.name);
                let target = Target::new(id, kind, &entry.name, entry.url.clone(), Source::Manual);
                state.upsert(target);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::parse;

    #[tokio::test]
    async fn discovers_manual_websites() {
        let yaml = r#"
manual:
  websites:
    - name: anthropic
      url: https://anthropic.com
"#;
        let config = parse(yaml).unwrap();
        let state = AppState::new();
        let d = ManualDiscoverer::new(&config);
        d.discover(&state).await.unwrap();
        assert_eq!(state.by_kind(TargetKind::Website).len(), 1);
    }
}
