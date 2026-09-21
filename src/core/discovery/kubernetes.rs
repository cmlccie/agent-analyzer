//! Kubernetes Service discovery.
//!
//! Watches Services matching the configured label selectors and maps them to
//! Targets using per-kind conventions (scheme=http, first port, base path).
//!
//! The base path is what the kind's prober starts from, not necessarily the path it requests:
//! `ModelProber` appends `/models` to it and `A2aProber` appends `/.well-known/agent-card.json`,
//! so those kinds get the API base (`/v1`, `/a2a`) rather than a fully-formed endpoint. Giving
//! them an endpoint instead yields `/v1/models/models` and a card path with no A2A prefix.
//!
//! Note: this module requires a live Kubernetes API server. It will fail to
//! initialize when running outside a cluster without `KUBECONFIG` set.

use crate::core::config::Config;
use crate::core::discovery::Discoverer;
use crate::core::state::AppState;
use crate::core::target::{Source, Target, TargetKind};
use anyhow::Context;
use k8s_openapi::api::core::v1::Service;
use kube::Client;
use kube::api::{Api, ListParams};
use url::Url;

pub struct KubernetesDiscoverer {
    client: Client,
    config: Config,
}

impl KubernetesDiscoverer {
    pub async fn new(config: &Config) -> anyhow::Result<Self> {
        let client = Client::try_default()
            .await
            .context("failed to build Kubernetes client")?;
        Ok(KubernetesDiscoverer {
            client,
            config: config.clone(),
        })
    }
}

impl Discoverer for KubernetesDiscoverer {
    async fn discover(&self, state: &AppState) -> anyhow::Result<()> {
        let selectors = &self.config.kubernetes.label_selectors;

        // (kind, selector, base path). Models take the OpenAI API base; agents take the base the
        // A2A app is mounted under, which is `/a2a` for fasta2a-derived servers; tools take the
        // MCP endpoint itself, since `McpProber` POSTs to the target URL unchanged.
        let kinds: &[(TargetKind, Option<&str>, &str)] = &[
            (TargetKind::Model, selectors.models.as_deref(), "/v1"),
            (TargetKind::Agent, selectors.agents.as_deref(), "/a2a"),
            (TargetKind::Tool, selectors.tools.as_deref(), "/mcp"),
        ];

        let namespaces: Vec<Option<&str>> = if self.config.kubernetes.namespaces.is_empty() {
            vec![None]
        } else {
            self.config
                .kubernetes
                .namespaces
                .iter()
                .map(|s| Some(s.as_str()))
                .collect()
        };

        for (kind, selector, base_path) in kinds {
            let Some(selector) = selector else {
                continue;
            };
            let lp = ListParams::default().labels(selector);

            for ns in &namespaces {
                let api: Api<Service> = match ns {
                    Some(ns) => Api::namespaced(self.client.clone(), ns),
                    None => Api::all(self.client.clone()),
                };

                let services = api
                    .list(&lp)
                    .await
                    .with_context(|| format!("listing services with '{selector}'"))?;

                for svc in services {
                    if let Some(target) = service_to_target(&svc, *kind, base_path) {
                        state.upsert(target);
                    }
                }
            }
        }

        Ok(())
    }
}

fn service_to_target(svc: &Service, kind: TargetKind, base_path: &str) -> Option<Target> {
    let meta = svc.metadata.clone();
    let name = meta.name?;
    let namespace = meta.namespace.unwrap_or_else(|| "default".to_string());

    let spec = svc.spec.as_ref()?;
    let port = spec
        .ports
        .as_ref()
        .and_then(|ports| ports.first())
        .map(|p| p.port)?;

    let url_str = format!("http://{name}.{namespace}.svc:{port}{base_path}");
    let url: Url = url_str.parse().ok()?;

    let id = format!("k8s-{namespace}-{name}");
    let mut target = Target::new(
        id,
        kind,
        &name,
        url,
        Source::Discovered {
            namespace: namespace.clone(),
            service: name.clone(),
        },
    );
    if let Some(labels) = meta.labels {
        target.metadata = labels.into_iter().collect();
    }
    Some(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{ServicePort, ServiceSpec};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    fn mock_service(name: &str, namespace: &str, port: i32) -> Service {
        Service {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(namespace.to_string()),
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                ports: Some(vec![ServicePort {
                    port,
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            status: None,
        }
    }

    #[test]
    fn service_to_target_model() {
        let svc = mock_service("vllm", "default", 8000);
        let t = service_to_target(&svc, TargetKind::Model, "/v1").unwrap();
        assert_eq!(t.name, "vllm");
        assert_eq!(t.url.to_string(), "http://vllm.default.svc:8000/v1");
        assert!(matches!(t.source, Source::Discovered { .. }));
    }

    #[test]
    fn service_without_ports_returns_none() {
        let mut svc = mock_service("empty", "default", 0);
        svc.spec.as_mut().unwrap().ports = None;
        assert!(service_to_target(&svc, TargetKind::Model, "/v1").is_none());
    }

    // A discovered target's URL is a base the prober extends, so the assertion that matters is
    // what the prober ends up requesting. Asserting only on the discovered URL is what let
    // `/v1/models` + an appended `/models` pass unnoticed.
    //
    // The lab's Services all publish port 80, which `url` normalizes out of an http:// URL as the
    // scheme default — hence no `:80` in the expected strings.
    #[test]
    fn discovered_urls_compose_into_valid_probe_urls() {
        let svc = mock_service("qwen3-coder-30b-sglang", "models", 80);
        let model = service_to_target(&svc, TargetKind::Model, "/v1").unwrap();
        assert_eq!(
            crate::core::probe::model::models_url(&model.url),
            "http://qwen3-coder-30b-sglang.models.svc/v1/models"
        );

        let svc = mock_service("weather-agent", "agents", 80);
        let agent = service_to_target(&svc, TargetKind::Agent, "/a2a").unwrap();
        assert_eq!(
            crate::core::probe::a2a::agent_card_url(&agent.url),
            "http://weather-agent.agents.svc/a2a/.well-known/agent-card.json"
        );

        let svc = mock_service("weather-mcp-server", "tools", 80);
        let tool = service_to_target(&svc, TargetKind::Tool, "/mcp").unwrap();
        assert_eq!(
            tool.url.to_string(),
            "http://weather-mcp-server.tools.svc/mcp"
        );
    }
}
