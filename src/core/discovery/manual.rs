use crate::core::config::Config;
use crate::core::discovery::Discoverer;
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
    async fn discover(&self) -> anyhow::Result<Vec<Target>> {
        let m = &self.config.manual;
        Ok([
            (TargetKind::Model, &m.models),
            (TargetKind::Agent, &m.agents),
            (TargetKind::Tool, &m.tools),
            (TargetKind::Website, &m.websites),
        ]
        .into_iter()
        .flat_map(|(kind, entries)| {
            entries.iter().map(move |entry| {
                let id = format!("manual-{kind}-{}", entry.name);
                Target::new(id, kind, &entry.name, entry.url.clone(), Source::Manual)
            })
        })
        .collect())
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
        let targets = ManualDiscoverer::new(&config).discover().await.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].kind, TargetKind::Website);
    }
}
