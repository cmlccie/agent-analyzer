use crate::cli::args::{ServeArgs, ServeMode};
use crate::core::config::{self, Config};
use crate::core::discovery;
use crate::core::mock;
use crate::core::probe;
use crate::core::server;
use crate::core::state::AppState;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
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
            let cfg = Arc::new(RwLock::new(config::load(&args.config)?));

            // Keep the watcher alive for the lifetime of serve.
            let _config_watcher = spawn_config_watcher(args.config.clone(), Arc::clone(&cfg))?;

            let probe_interval = cfg.read().unwrap().general.probe_interval();
            let http_timeout = cfg.read().unwrap().general.http_timeout();
            let discovery_interval = cfg.read().unwrap().general.discovery_interval();

            // Initial discovery pass.
            {
                let snapshot = cfg.read().unwrap().clone();
                discovery::run_once(&snapshot, &state).await?;
            }

            // Background discovery scheduler — reads the live config Arc each tick
            // so label-selector and namespace changes take effect without a restart.
            let discovery_state = state.clone();
            let discovery_cfg = Arc::clone(&cfg);
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(discovery_interval);
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    let snapshot = discovery_cfg.read().expect("config lock poisoned").clone();
                    if let Err(e) = discovery::run_once(&snapshot, &discovery_state).await {
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

/// Watch the config file's parent directory for changes and atomically update
/// the shared config Arc.
///
/// The parent directory (not the file itself) is watched to handle the K8s
/// ConfigMap symlink-swap pattern: when a ConfigMap is updated, Kubernetes
/// atomically renames the backing `..data` directory, which triggers a RENAME
/// event on the parent rather than a MODIFY on the file path.
fn spawn_config_watcher(
    config_path: PathBuf,
    config: Arc<RwLock<Config>>,
) -> anyhow::Result<RecommendedWatcher> {
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = RecommendedWatcher::new(tx, notify::Config::default())?;

    let watch_dir = config_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;

    std::thread::spawn(move || {
        for res in &rx {
            match res {
                Ok(_event) => {
                    if !config_path.exists() {
                        continue;
                    }
                    match config::load(&config_path) {
                        Ok(new_cfg) => {
                            tracing::info!("config reloaded from {}", config_path.display());
                            *config.write().expect("config lock poisoned") = new_cfg;
                        }
                        Err(e) => tracing::warn!("config reload failed: {e}"),
                    }
                }
                Err(e) => tracing::warn!("file watcher error: {e}"),
            }
        }
    });

    Ok(watcher)
}
