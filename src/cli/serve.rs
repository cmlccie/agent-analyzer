use crate::cli::args::{ServeArgs, ServeMode};
use crate::core::config;
use crate::core::discovery;
use crate::core::mock;
use crate::core::probe;
use crate::core::server;
use crate::core::state::AppState;
use tokio::time::Duration;

pub async fn run(args: ServeArgs) -> anyhow::Result<()> {
    let state = AppState::new();

    match args.mode {
        Some(ServeMode::Mock(mock_args)) => {
            tracing::info!("starting in mock mode");
            mock::populate(&state, &mock_args.fail);
            server::serve(args.bind, state).await?;
        }
        None => {
            tracing::info!("loading config from {}", args.config.display());
            let cfg = config::load(&args.config)?;
            let probe_interval = cfg.general.probe_interval();
            let http_timeout = cfg.general.http_timeout();

            // Initial discovery pass.
            discovery::run_once(&cfg, &state).await?;

            // Background discovery scheduler.
            let discovery_state = state.clone();
            let discovery_cfg = cfg.clone();
            let discovery_interval = cfg.general.discovery_interval();
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(discovery_interval);
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    if let Err(e) = discovery::run_once(&discovery_cfg, &discovery_state).await {
                        tracing::warn!("discovery failed: {e}");
                    }
                }
            });

            // Background probe scheduler.
            probe::spawn_scheduler(
                state.clone(),
                Duration::from_secs(probe_interval.as_secs()),
                Duration::from_secs(http_timeout.as_secs()),
            );

            server::serve(args.bind, state).await?;
        }
    }

    Ok(())
}
