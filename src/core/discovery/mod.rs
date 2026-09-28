pub mod kubernetes;
pub mod manual;

use crate::core::config::Config;
use crate::core::state::AppState;
use crate::core::target::Target;

/// Trait implemented by each discovery backend.
#[allow(async_fn_in_trait)]
pub trait Discoverer {
    /// Run a single discovery pass and return every target currently found.
    async fn discover(&self) -> anyhow::Result<Vec<Target>>;
}

/// Run all enabled discoverers once and reconcile `state` with the result.
///
/// Targets no discoverer returned are removed. If any discoverer fails, the pass is abandoned
/// before reconciling, so a Kubernetes API outage does not empty the inventory.
pub async fn run_once(config: &Config, state: &AppState) -> anyhow::Result<()> {
    let mut targets = manual::ManualDiscoverer::new(config).discover().await?;

    if config.kubernetes.enabled {
        let kube = kubernetes::KubernetesDiscoverer::new(config).await?;
        targets.extend(kube.discover().await?);
    }

    state.reconcile(targets);
    Ok(())
}
