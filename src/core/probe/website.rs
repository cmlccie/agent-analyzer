use crate::core::probe::{Prober, failed_status};
use crate::core::target::{Status, Target};
use chrono::Utc;
use reqwest::Client;
use serde_json::json;

pub struct WebsiteProber {
    http: Client,
}

impl WebsiteProber {
    pub fn new(http: Client) -> Self {
        WebsiteProber { http }
    }
}

impl Prober for WebsiteProber {
    async fn probe(&self, target: &Target) -> Status {
        match self.http.get(target.url.as_str()).send().await {
            Ok(resp) => {
                let status_code = resp.status().as_u16();
                if resp.status().is_success() || resp.status().is_redirection() {
                    Status::Ok {
                        details: json!({ "status_code": status_code }),
                        checked_at: Utc::now(),
                    }
                } else {
                    failed_status(format!("HTTP {status_code}"))
                }
            }
            Err(e) => failed_status(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::target::{Source, TargetKind};

    fn make_target(url: &str) -> Target {
        Target::new(
            "w1",
            TargetKind::Website,
            "test",
            url.parse().unwrap(),
            Source::Manual,
        )
    }

    #[tokio::test]
    async fn unreachable_url_returns_failed() {
        let http = Client::builder()
            .timeout(std::time::Duration::from_millis(100))
            .build()
            .unwrap();
        let prober = WebsiteProber::new(http);
        let target = make_target("http://127.0.0.1:1"); // nothing listening
        let status = prober.probe(&target).await;
        assert!(!status.is_ok());
    }
}
