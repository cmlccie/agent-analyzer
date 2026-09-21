# Stage 1 — compile a fully-static binary using the Alpine (musl) Rust toolchain.
# The CI workflow fans linux/amd64 and linux/arm64 out to native runners
# (ubuntu-24.04 and ubuntu-24.04-arm), so neither arch is emulated and each
# builder targets its own musl triple automatically.
#
# The Rust version is pinned deliberately: a floating `rust:1-alpine` tag moves
# on every Rust release, changing the base layer and invalidating the entire
# dependency cache on both arches at an unpredictable moment. Bump this on
# purpose, not by surprise.
#
# DO NOT add BuildKit `--mount=type=cache` for ~/.cargo or target/ here.
# Cache mounts are not exported by the gha/registry/local cache backends, so in
# CI (fresh builder per job) they would cache nothing. The layer cache below is
# what actually makes this fast.
FROM rust:1.98-alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /build

# Pre-compile all dependencies in their own layer, keyed only on the manifests,
# so that changes under src/ do not trigger a dependency rebuild.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo 'pub mod core { pub mod errors { pub type Error = Box<dyn std::error::Error + Send + Sync>; pub type Result<T> = std::result::Result<T, Error>; } }' > src/lib.rs && \
    echo 'fn main() {}' > src/main.rs && \
    cargo build --release --locked && \
    rm -rf src

COPY . .
# Touch the entry points so Cargo sees them as newer than the cached stubs.
RUN touch src/lib.rs src/main.rs && cargo build --release --locked

# Stage 2 — distroless runtime: no shell, no package manager, non-root by default.
FROM gcr.io/distroless/static-debian12:nonroot

COPY --from=builder /build/target/release/agent-analyzer /opt/agent-analyzer

WORKDIR /opt
EXPOSE 8000

CMD ["/opt/agent-analyzer", "serve", "--config", "/etc/agent-analyzer/config.yaml"]
