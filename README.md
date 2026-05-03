# Agent Analyzer

A Kubernetes-native terminal UI for discovering and probing AI components — models, agents, tools, and websites — with continuous reachability monitoring.

## Overview

`agent-analyzer` runs as a lightweight backend server that:

- **Discovers** AI components from Kubernetes services (by label selector) and from a static config file
- **Probes** each target on a configurable interval using the appropriate protocol: OpenAI-compatible API, MCP JSON-RPC handshake, Agent-to-Agent (A2A), or plain HTTP
- **Exposes** current state over a local HTTP API

A TUI client polls the backend and displays live, color-coded status across three panes (Models / Agents / Tools).

## Usage

```bash
# Start the backend server
agent-analyzer serve --config /etc/agent-analyzer/config.yaml

# Launch the terminal UI (connects to server at localhost:8000 by default)
agent-analyzer tui

# Print a one-shot status snapshot
agent-analyzer status
agent-analyzer status --json
```

Run both components locally for development:

```bash
# Terminal 1 — start the server with mock data (no K8s or live endpoints needed)
make mock

# Terminal 2 — launch the TUI or check status
cargo run -- tui
make status
```

## Configuration

The server reads a YAML config file. Example:

```yaml
general:
  discovery_interval_secs: 30
  probe_interval_secs: 10
  http_timeout_secs: 5

kubernetes:
  enabled: true
  namespaces: [] # empty = all accessible namespaces
  label_selectors:
    models: "app=vllm"
    agents: "app=a2a-agent"
    tools: "app in (mcp-fs, mcp-github)"

manual:
  websites:
    - name: anthropic.com
      url: https://anthropic.com
  models: []
  agents: []
  tools: []
```

Config changes are picked up at runtime without a restart (hot-reload via file watcher, compatible with Kubernetes ConfigMap symlink swaps).

## Development

```bash
make build    # cargo build
make release  # cargo build --release
make check    # fmt check + clippy + tests
make test     # cargo test
make lint     # cargo fmt --check && cargo clippy -D warnings
make fmt      # cargo fmt
make clean    # cargo clean
```

## Deploying to Kubernetes

The `deploy/` directory contains ready-to-use manifests:

| File               | Purpose                                                                      |
| ------------------ | ---------------------------------------------------------------------------- |
| `configmap.yaml`   | Server config mounted at `/etc/agent-analyzer/config.yaml`                   |
| `deployment.yaml`  | Single-replica Deployment + Service (port 8000)                              |
| `rbac.yaml`        | ServiceAccount + ClusterRole with read-only access to Services and Endpoints |
| `netpol-demo.yaml` | Example NetworkPolicy                                                        |

```bash
kubectl apply -f deploy/rbac.yaml
kubectl apply -f deploy/configmap.yaml
kubectl apply -f deploy/deployment.yaml
```

The container image is built from `Containerfile` as a fully-static musl binary on a distroless base (no shell, non-root). Multi-arch builds (amd64/arm64) work without cross-compilation.

## API

The server exposes a REST API on port 8000:

| Endpoint                   | Description                       |
| -------------------------- | --------------------------------- |
| `GET /api/v1/healthz`      | Health check                      |
| `GET /api/v1/state`        | Full state snapshot               |
| `GET /api/v1/targets`      | List targets (filterable by kind) |
| `GET /api/v1/targets/{id}` | Single target detail              |
