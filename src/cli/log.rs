use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Initialise the tracing subscriber.
///
/// `verbosity` maps CLI `-v` count to a log level:
/// - 0 → WARN
/// - 1 → INFO
/// - 2 → DEBUG
/// - 3+ → TRACE
pub fn init(verbosity: u8) {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    // RUST_LOG overrides the verbosity flag.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(filter)
        .init();
}
