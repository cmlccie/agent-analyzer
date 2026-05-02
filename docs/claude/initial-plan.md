# Plan: `agent-analyzer` Rust TUI

## Context

Build a Kubernetes-native Rust binary that discovers and probes AI components (LLM inference endpoints, MCP servers, A2A agents) reachable from the pod it runs in, and visualizes their reachability in a TUI. Two purposes:

1. **Discovery / troubleshooting** — see at a glance what models/agents/tools your environment can talk to.
2. **Cilium NetworkPolicy demo** — apply a policy and watch previously-green checkmarks flip to red X's in real time.

The repo is currently an empty `cargo new` skeleton (`Cargo.toml` has `name = "agent-analyzer"`, edition `2024`, no dependencies; no `src/` files). This plan establishes the full crate structure, dependencies, and module layout, mirroring the patterns used in [`cmlccie/awsipranges`](https://github.com/cmlccie/awsipranges) (lib + bin, `core/` and `cli/` modules, embedded unit tests, `tests/` integration suite).

---

## Confirmed design decisions

- **TUI ↔ serve transport**: serve binds `0.0.0.0:8000` (CLI-overridable). tui/status default to `http://localhost:8000`. No auth in v1; K8s controls exposure.
- **MCP / A2A**: try `mcp-client-rs` and `fasa2a` upstream first; abstract behind an internal trait so we can swap to a thin in-house layer if either crate proves problematic.
- **Hot reload**: `notify` crate watching `config.yaml`. Handle the K8s ConfigMap symlink-swap pattern.
- **Health probes**: protocol-native (`/v1/models` via async-openai; MCP `initialize` + `tools/list`; A2A agent-card fetch; HTTP GET for websites). Required for the Cilium demo to be meaningful.
- **K8s discovery**: convention-only, with **user-supplied K8s-native label selectors** per component type — no `agent-analyzer.io/*` labels are required on target services. Operators write whatever selector matches their existing labels (`app=vllm`, `model=gpt-oss-20b`, `app in (mcp-fs, mcp-github)`, etc.). For each matched Service we apply per-kind conventions: scheme=http, first/named port, default path (`/v1/models` for model, `/mcp` for MCP tool, `/.well-known/agent.json` for A2A agent). No annotations, no CRDs.
- **Manual vs discovered**: same `Target` struct with `source: Discovered | Manual` enum; merged into the same pane lists.
- **Mock**: `serve mock` loads fixed in-code fixtures; `--fail <kind:name>` flag flips selected items to failed for demo dry-runs.

---

## Crate layout

```
agent-analyzer/
├── Cargo.toml
├── README.md
├── Containerfile
├── .github/workflows/image.yaml
├── deploy/
│   ├── configmap.yaml          # sample config
│   ├── deployment.yaml
│   ├── rbac.yaml               # SA + read-only Role/RoleBinding
│   └── netpol-demo.yaml        # CiliumNetworkPolicy demo
├── src/
│   ├── lib.rs                  # public API + Error/Result aliases
│   ├── main.rs                 # thin: parse Args → dispatch
│   ├── core/
│   │   ├── mod.rs
│   │   ├── config.rs           # serde model + load/watch
│   │   ├── target.rs           # Target, TargetKind, Source, Status
│   │   ├── state.rs            # Arc<RwLock<AppState>> shared between server & probers
│   │   ├── discovery/
│   │   │   ├── mod.rs          # trait Discoverer
│   │   │   ├── kubernetes.rs   # kube-rs Service watcher, label-selected
│   │   │   └── manual.rs       # config-driven static targets
│   │   ├── probe/
│   │   │   ├── mod.rs          # trait Prober + scheduler
│   │   │   ├── model.rs        # async-openai /v1/models
│   │   │   ├── mcp.rs          # mcp-client-rs initialize+tools/list
│   │   │   ├── a2a.rs          # fasa2a agent-card fetch
│   │   │   └── website.rs      # reqwest GET
│   │   ├── server/
│   │   │   ├── mod.rs          # axum app; routes
│   │   │   ├── routes.rs       # GET /api/v1/{state,models,agents,tools,websites,target/:id}
│   │   │   └── api_types.rs    # serde DTOs shared with client
│   │   ├── client.rs           # reqwest-based typed client used by tui/status
│   │   ├── mock.rs             # fixed fixtures + --fail handling
│   │   ├── errors.rs
│   │   └── utils.rs
│   └── cli/
│       ├── mod.rs              # pub use Args
│       ├── args.rs             # clap Parser; subcommands Serve, Tui, Status
│       ├── log.rs              # tracing-subscriber setup
│       ├── serve.rs            # orchestrates discovery + probes + server
│       ├── tui/
│       │   ├── mod.rs          # event loop, layout
│       │   ├── widgets.rs      # left panes, detail pane, websites bar, header
│       │   └── ascii.rs        # banner
│       └── status.rs           # one-shot status print (text/json)
└── tests/
    ├── status_integration.rs   # spawn `serve mock` + run `status`
    └── api_integration.rs      # spawn `serve mock` + hit HTTP API
```

### Key types (sketch)

```rust
// core/target.rs
pub enum TargetKind { Model, Agent, Tool, Website }
pub enum Source { Discovered { namespace: String, service: String }, Manual }
pub enum Status { Unknown, Ok { details: serde_json::Value, checked_at: DateTime<Utc> },
                  Failed { error: String, checked_at: DateTime<Utc> } }
pub struct Target { pub id: String, pub kind: TargetKind, pub name: String,
                    pub url: Url, pub source: Source, pub status: Status,
                    pub metadata: BTreeMap<String, String> }
```

`AppState` = `Arc<RwLock<HashMap<TargetId, Target>>>`, owned by serve, mutated by discovery + probe tasks, read by HTTP handlers.

### HTTP API (serve)

- `GET /api/v1/state` — full snapshot (used by `status` and tui initial load)
- `GET /api/v1/targets?kind=model|agent|tool|website` — filtered list
- `GET /api/v1/targets/:id` — single target detail
- `GET /api/v1/healthz` — liveness (always 200 if process up)
- (later) `GET /api/v1/events` — SSE stream so tui can push-update instead of polling

For v1, tui polls `/api/v1/state` every ~1s; SSE is a follow-up.

---

## Dependencies (Cargo.toml)

```toml
[dependencies]
# CLI / logging
clap = { version = "4", features = ["derive", "wrap_help"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Async runtime
tokio = { version = "1", features = ["full"] }
futures = "0.3"

# Config + serde
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
serde_json = "1"
notify = "6"

# HTTP server / client
axum = "0.7"
tower = "0.5"
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
url = { version = "2", features = ["serde"] }

# K8s
kube = { version = "0.95", features = ["runtime", "rustls-tls"], default-features = false }
k8s-openapi = { version = "0.23", features = ["latest"] }

# Protocol probes
async-openai = "0.27"
mcp-client-rs = "*"   # pin once we verify version compatibility
fasa2a       = "*"

# TUI
ratatui = "0.28"
crossterm = "0.28"

# Misc
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
thiserror = "2"

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"
tokio = { version = "1", features = ["full", "test-util"] }
```

Error pattern (matching awsipranges): `pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>; pub type Result<T> = std::result::Result<T, Error>;` in `lib.rs`, with `thiserror`-derived domain errors inside modules.

---

## CLI surface

```
agent-analyzer                       # default → tui --url http://localhost:8000
agent-analyzer tui [--url URL]
agent-analyzer status [--url URL] [--format text|json]
agent-analyzer serve --config /etc/agent-analyzer/config.yaml [--bind 0.0.0.0:8000]
agent-analyzer serve mock [--fail KIND:NAME]... [--bind 0.0.0.0:8000]
```

---

## TUI layout

```
┌─────────────────────────────────────────────────────────────┐
│   █▀█ █▀▀ █▀▀ █▄ █ ▀█▀  ▄▀█ █▄ █ ▄▀█ █▄ █▀█        keys:   │
│   █▀█ █▄█ ██▄ █ ▀█  █   █▀█ █ ▀█ █▀█ █ ▀█▄█        q tab ↑↓│
├──────────────────┬──────────────────────────────────────────┤
│ Models           │ Detail: <selected target>                │
│  ✓ gpt-4 (svc)   │ URL:  http://...                          │
│  ✗ llama (man)   │ Source: Discovered ns/svc                 │
├──────────────────┤ Last check: 2026-05-02T17:42:01Z          │
│ Agents           │ Probe result:                             │
│  ✓ planner       │   models: [...]                           │
├──────────────────┤                                           │
│ Tools (MCP)      │                                           │
│  ✓ filesystem    │                                           │
│  ✗ github        │                                           │
├──────────────────┴──────────────────────────────────────────┤
│ Websites:  ✓ anthropic.com   ✗ openai.com   ✓ github.com    │
└─────────────────────────────────────────────────────────────┘
```

`Tab`/`Shift-Tab` cycles panes; `↑`/`↓` selects within pane; `q` quits; `r` forces probe refresh (POST /api/v1/refresh — stretch).

---

## Configuration schema (config.yaml)

```yaml
general:
  discovery_interval: 30s
  probe_interval: 10s
  http_timeout: 5s

kubernetes:
  enabled: true
  namespaces: [] # empty = all accessible
  # Standard Kubernetes label-selector syntax — operators target whatever
  # labels their existing services already use. Equality, set, and comma-AND
  # forms are all supported (parsed as a single string per kind).
  label_selectors:
    models: "app=vllm"
    agents: "app=a2a-agent"
    tools: "app in (mcp-fs, mcp-github)"

manual:
  websites:
    - name: anthropic
      url: https://anthropic.com
  models: []
  agents: []
  tools: []
```

Loaded at startup; `notify` watcher re-parses on change and atomically swaps the config Arc. Discovery + probe schedulers read the live Arc each tick.

---

## Deployment artifacts

- **Containerfile**: multi-stage. Stage 1 `rust:1-alpine` or `cargo-chef` for caching, build with `--target x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` for static binary. Stage 2 `gcr.io/distroless/static-debian12:nonroot`. Binary at `/opt/agent-analyzer` (short path keeps demo typing fast), default config path `/etc/agent-analyzer/config.yaml`, `WORKDIR /opt`. `CMD ["/opt/agent-analyzer", "serve", "--config", "/etc/agent-analyzer/config.yaml"]`.
- **GitHub Actions workflow**: `docker/setup-buildx-action` + `docker/build-push-action` with `platforms: linux/amd64,linux/arm64`, push to GHCR.
- **deploy/rbac.yaml**: ServiceAccount + ClusterRole granting `get,list,watch` on `services` and `endpoints` (no secrets). RoleBinding scoped to namespaces or ClusterRoleBinding per ops choice.
- **deploy/deployment.yaml**: 1 replica, mounts the ConfigMap at `/etc/agent-analyzer/`, container port 8000.
- **deploy/netpol-demo.yaml**: CiliumNetworkPolicy that allows egress to a subset of model/agent/tool service labels — used to demo the green→red transitions.

---

## Verification

End-to-end checks for the implementer to run:

1. `cargo build --release` — builds clean on stable toolchain (edition 2024 requires Rust ≥1.85).
2. `cargo clippy --all-targets --all-features -- -D warnings` — no lint warnings; CI gates on this.
3. `cargo fmt --check` — formatting clean.
4. `cargo test` — all module unit tests + integration tests pass.
5. `cargo run -- serve mock` then in another terminal `cargo run -- status` → status command lists fixtures with the expected ✓/✗.
6. `cargo run -- serve mock --fail model:gpt-4` then `cargo run` → TUI shows red X next to gpt-4, green elsewhere.
7. `curl http://localhost:8000/api/v1/state | jq` against `serve mock` returns the full state JSON.
8. Containerfile build: `docker buildx build --platform linux/amd64,linux/arm64 -t agent-analyzer:dev .` succeeds.
9. In-cluster smoke test: deploy via `deploy/`, point selectors at a couple of existing Services (e.g. `app=vllm`), verify discovery picks them up (`kubectl exec -it deploy/agent-analyzer -- /opt/agent-analyzer status`), then apply `deploy/netpol-demo.yaml` and verify selected targets flip to ✗ within one probe interval.

---

## Implementation order (suggested)

1. Crate skeleton + Cargo.toml + lib.rs/main.rs with empty `core` and `cli` modules; CI builds.
2. `core::target`, `core::config`, `core::state` + unit tests.
3. `core::server` + `core::client` + `cli::status` against an empty state.
4. `core::mock` + `serve mock` subcommand + first integration test (`tests/status_integration.rs`).
5. `core::probe::website` + scheduler — first real probe.
6. `core::probe::model` (async-openai), then MCP, then A2A.
7. `core::discovery::kubernetes` (kube-rs Service watcher).
8. `notify`-driven config hot reload.
9. `cli::tui` widgets + event loop.
10. Containerfile, GitHub Actions, deploy manifests, NetworkPolicy demo.

Each step keeps the binary buildable and the test suite green.
