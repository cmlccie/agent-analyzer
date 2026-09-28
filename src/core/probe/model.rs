//! Model endpoint probe — lists served models via the OpenAI-compatible `GET <base>/models`.

use crate::core::probe::{Prober, failed_status, http_failed, request_failed};
use crate::core::target::{FailureKind, Status, Target};
use chrono::Utc;
use reqwest::Client;
use serde_json::{Value, json};

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
            Ok(resp) if resp.status().is_success() => match resp.json::<Value>().await {
                Ok(body) => match model_ids(&body) {
                    Some(models) => Status::Ok {
                        details: json!({ "models": models }),
                        checked_at: Utc::now(),
                    },
                    None => failed_status(FailureKind::Protocol, "response has no model list"),
                },
                Err(e) => failed_status(FailureKind::Protocol, format!("invalid JSON: {e}")),
            },
            Ok(resp) => http_failed(resp.status()),
            Err(e) => request_failed(e),
        }
    }
}

/// Model ids from an OpenAI-style list response (`{"data": [{"id": ...}, ...]}`).
fn model_ids(body: &Value) -> Option<Vec<String>> {
    body["data"].as_array().map(|models| {
        models
            .iter()
            .filter_map(|m| m["id"].as_str().map(String::from))
            .collect()
    })
}

/// Resolve the `/models` URL relative to the target's base URL, which is the OpenAI API base
/// (`.../v1`) — not an endpoint. A base that already names an endpoint yields `/v1/models/models`.
pub(crate) fn models_url(base: &url::Url) -> String {
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
    fn model_ids_from_openai_list() {
        let body = json!({"object": "list", "data": [{"id": "gemma-4"}, {"id": "llama-3"}]});
        assert_eq!(model_ids(&body).unwrap(), ["gemma-4", "llama-3"]);
        assert!(model_ids(&json!({"hello": "world"})).is_none());
    }

    #[test]
    fn models_url_no_double_slash() {
        let base: url::Url = "http://localhost:8000/v1/".parse().unwrap();
        let url = models_url(&base);
        assert!(!url.contains("//models"), "got: {url}");
    }
}
