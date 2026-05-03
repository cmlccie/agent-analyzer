# Stage 1 — compile a fully-static binary using the Alpine (musl) Rust toolchain.
# docker/build-push-action with platforms: linux/amd64,linux/arm64 spawns two
# independent native builders (or QEMU-emulated), so there is no cross-compilation
# complexity; each builder targets its own musl triple automatically.
FROM rust:1-alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /build

# Pre-compile all dependencies so rebuilds triggered by src/ changes are fast.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo 'pub mod core { pub mod errors { pub type Error = Box<dyn std::error::Error + Send + Sync>; pub type Result<T> = std::result::Result<T, Error>; } }' > src/lib.rs && \
    echo 'fn main() {}' > src/main.rs && \
    cargo build --release || true && \
    rm -rf src

COPY . .
# Touch the entry points so Cargo sees them as newer than the cached stubs.
RUN touch src/lib.rs src/main.rs && cargo build --release

# Stage 2 — distroless runtime: no shell, no package manager, non-root by default.
FROM gcr.io/distroless/static-debian12:nonroot

COPY --from=builder /build/target/release/agent-analyzer /opt/agent-analyzer

WORKDIR /opt
EXPOSE 8000

CMD ["/opt/agent-analyzer", "serve", "--config", "/etc/agent-analyzer/config.yaml"]
