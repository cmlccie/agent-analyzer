use crate::core::server::api_types::{StateResponse, TargetResponse};
use crate::core::target::Target;
use anyhow::Context;
use reqwest::Client;
use url::Url;

/// HTTP client for the `serve` backend, used by `tui` and `status`.
pub struct ApiClient {
    base: Url,
    http: Client,
}

impl ApiClient {
    pub fn new(base_url: Url) -> anyhow::Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("failed to build HTTP client")?;
        Ok(ApiClient {
            base: base_url,
            http,
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base
    }

    /// Fetch a full state snapshot from the server.
    pub async fn state(&self) -> anyhow::Result<Vec<Target>> {
        let url = self.url("/api/v1/state");
        let resp: StateResponse = self
            .http
            .get(url)
            .send()
            .await
            .context("GET /api/v1/state")?
            .error_for_status()
            .context("server returned error")?
            .json()
            .await
            .context("deserializing state response")?;
        Ok(resp.targets)
    }

    /// Fetch a single target by id.
    pub async fn target(&self, id: &str) -> anyhow::Result<Target> {
        let url = self.url(&format!("/api/v1/targets/{id}"));
        let resp: TargetResponse = self
            .http
            .get(url)
            .send()
            .await
            .with_context(|| format!("GET /api/v1/targets/{id}"))?
            .error_for_status()
            .context("server returned error")?
            .json()
            .await
            .context("deserializing target response")?;
        Ok(resp.target)
    }

    /// Liveness check — returns `true` if the server is reachable.
    pub async fn healthz(&self) -> bool {
        let url = self.url("/api/v1/healthz");
        self.http.get(url).send().await.is_ok()
    }

    fn url(&self, path: &str) -> Url {
        let mut u = self.base.clone();
        u.set_path(path);
        u
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_new_ok() {
        let c = ApiClient::new("http://localhost:8000".parse().unwrap()).unwrap();
        assert_eq!(c.base_url().host_str(), Some("localhost"));
    }
}
