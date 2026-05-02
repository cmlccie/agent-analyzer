use thiserror::Error;

/// Crate-level error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level error alias — box-erased for public API compatibility.
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Domain errors produced by `core` subsystems.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("discovery error: {0}")]
    Discovery(String),

    #[error("probe failed for '{target}': {reason}")]
    Probe { target: String, reason: String },

    #[error("server error: {0}")]
    Server(String),

    #[error("client error: {0}")]
    Client(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_error_display() {
        let e = CoreError::Config("missing field".into());
        assert!(e.to_string().contains("missing field"));

        let e = CoreError::Probe {
            target: "gpt-4".into(),
            reason: "connection refused".into(),
        };
        assert!(e.to_string().contains("gpt-4"));
        assert!(e.to_string().contains("connection refused"));
    }
}
