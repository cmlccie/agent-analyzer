//! Model endpoint probe — queries `/v1/models` via the OpenAI-compatible API.
//!
//! Uses `async-openai` (added in step 6). For now the prober falls back to a
//! plain HTTP GET on the target URL so the skeleton compiles without a
//! fully wired OpenAI client.

use crate::core::probe::{Prober, failed_status};
use crate::core::target::{Status, Target};
use chrono::Utc;
use reqwest::Client;
use serde_json::json;

pub struct ModelProber {
    http: Client,
}

impl ModelProber {
    pub fn new(http: Client) -> Self {
        ModelProber { http }
    }
}

impl Prober for ModelProber {
    async fn probe(&self, target: &Target) -> Status {
        // Build the /v1/models URL relative to the target base.
        let models_url = models_url(&target.url);

        match self.http.get(models_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await.unwrap_or(json!({}));
                Status::Ok {
                    details: body,
                    checked_at: Utc::now(),
                }
            }
            Ok(resp) => failed_status(format!("HTTP {}", resp.status())),
            Err(e) => failed_status(e),
        }
    }
}

fn models_url(base: &url::Url) -> String {
    let mut u = base.clone();
    let path = u.path().trim_end_matches('/').to_string();
    u.set_path(&format!("{path}/models"));
    u.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_url_appends_models() {
        let base: url::Url = "http://localhost:8000/v1".parse().unwrap();
        let url = models_url(&base);
        assert!(url.ends_with("/v1/models"), "got: {url}");
    }

    #[test]
    fn models_url_no_double_slash() {
        let base: url::Url = "http://localhost:8000/v1/".parse().unwrap();
        let url = models_url(&base);
        assert!(!url.contains("//models"), "got: {url}");
    }
}
