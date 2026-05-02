use crate::core::mock::FailKey;
use clap::{Parser, Subcommand};

/// Kubernetes-native AI component discovery and reachability analyzer.
#[derive(Debug, Parser)]
#[command(name = "agent-analyzer", version, about, long_about = None)]
pub struct Args {
    /// Increase log verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the backend server (discovery + probing + HTTP API).
    Serve(ServeArgs),
    /// Launch the terminal UI (default when no sub-command is given).
    Tui(TuiArgs),
    /// Print a point-in-time status snapshot and exit.
    Status(StatusArgs),
}

#[derive(Debug, Parser)]
pub struct ServeArgs {
    /// Path to the configuration file.
    #[arg(long, default_value = "/etc/agent-analyzer/config.yaml")]
    pub config: std::path::PathBuf,

    /// Address to listen on.
    #[arg(long, default_value = "0.0.0.0:8000")]
    pub bind: std::net::SocketAddr,

    #[command(subcommand)]
    pub mode: Option<ServeMode>,
}

#[derive(Debug, Subcommand)]
pub enum ServeMode {
    /// Run with fixed mock data instead of live discovery and probing.
    Mock(MockArgs),
}

#[derive(Debug, Parser)]
pub struct MockArgs {
    /// Force specific targets to a failed state, e.g. `--fail model:gpt-4`.
    #[arg(long = "fail", value_name = "KIND:NAME")]
    pub fail: Vec<FailKey>,
}

#[derive(Debug, Parser)]
pub struct TuiArgs {
    /// Backend server URL.
    #[arg(long, default_value = "http://localhost:8000")]
    pub url: url::Url,
}

#[derive(Debug, Parser)]
pub struct StatusArgs {
    /// Backend server URL.
    #[arg(long, default_value = "http://localhost:8000")]
    pub url: url::Url,

    /// Output format.
    #[arg(long, default_value = "text", value_enum)]
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}
