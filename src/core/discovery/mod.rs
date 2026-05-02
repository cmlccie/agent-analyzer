pub mod kubernetes;
pub mod manual;

use crate::core::config::Config;
use crate::core::state::AppState;

/// Trait implemented by each discovery backend.
#[allow(async_fn_in_trait)]
pub trait Discoverer {
    /// Run a single discovery pass, upserting found targets into `state`.
    async fn discover(&self, state: &AppState) -> anyhow::Result<()>;
}

/// Run all enabled discoverers once.
pub async fn run_once(config: &Config, state: &AppState) -> anyhow::Result<()> {
    let manual = manual::ManualDiscoverer::new(config);
    manual.discover(state).await?;

    if config.kubernetes.enabled {
        let kube = kubernetes::KubernetesDiscoverer::new(config).await?;
        kube.discover(state).await?;
    }

    Ok(())
}
