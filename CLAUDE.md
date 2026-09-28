# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                  # build
cargo test                   # run all tests
cargo test <test_name>       # run a single test
cargo clippy                 # lint
cargo fmt                    # format
cargo run                    # run backend server (default)
cargo run -- tui             # launch terminal UI
cargo run -- status          # print point-in-time snapshot
cargo run -- status --json   # JSON output
```

## Architecture

`agent-analyzer` is a Kubernetes-native TUI application for discovering and probing AI components (models, agents, tools, websites) with reachability testing. It runs as a backend server that continuously discovers and probes targets, with a TUI client that polls the backend.

### Module layout

**`src/core/`** — Domain logic and backend services
- `target.rs` — Core types: `Target`, `TargetKind` (Model/Agent/Tool/Website), `Status` (Ok/Failed/Unknown), `Source` (Manual or Kubernetes-discovered)
- `state.rs` — Thread-safe `AppState` (`Arc<RwLock<HashMap>>`) shared across async tasks; provides upsert/remove/all/by_kind/get
- `discovery/` — `Discoverer` trait implementations: `kubernetes.rs` (watches K8s services by label selector), `manual.rs` (static YAML config)
- `probe/` — `Prober` trait implementations: `model.rs` (OpenAI-compatible `/v1/models`), `mcp.rs` (MCP Streamable HTTP handshake + `tools/list`), `a2a.rs` (A2A agent card + skills), `website.rs` (HTTP reachability); `mod.rs` classifies failures into `FailureKind`
- `server/` — Axum HTTP API (`/api/v1/healthz`, `/api/v1/state`, `/api/v1/targets`, `/api/v1/targets/{id}`, `POST /api/v1/refresh`)
- `config.rs` — YAML config loading: intervals, K8s namespaces/selectors, manual target lists
- `client.rs` — HTTP client for TUI → backend communication
- `mock.rs` — Mock data for testing without live infrastructure

**`src/cli/`** — User interfaces
- `serve.rs` — Backend orchestration: loads config, spawns discovery scheduler, probe scheduler, serves HTTP API; supports hot-reload via file watcher (K8s ConfigMap symlink-swap compatible)
- `tui/` — Ratatui + Crossterm TUI: polls backend, four inventory panes (Models/Agents (A2A)/Tools (MCP)/External Hosts) over a detail pane, color-coded status
- `status.rs` — One-shot CLI snapshot (text or JSON)
- `args.rs` — Clap CLI parsing

### Key patterns

- **Trait-based extensibility**: New discovery sources implement `Discoverer`; new protocol probers implement `Prober`
- **Discovery reconciles, probes record**: discoverers return their full target set and `AppState::reconcile` replaces the inventory (pruning what is gone, keeping probe status/history); probes only call `AppState::record`
- **Shared state**: `AppState` is the single source of truth, passed via `Arc` to all background tasks and the HTTP server
- **Background schedulers**: Discovery and probing run on tokio-spawned loops with configurable intervals from config
- **Error handling**: `anyhow::Result` throughout; `thiserror` for domain error types
