# syntax=docker/dockerfile:1

# ---------------------------------------------------------------------------
# Build
#
# Two toolchain versions are pinned together on purpose: the wasm-bindgen schema
# is unstable between patch releases, so the CLI must match the `wasm-bindgen`
# crate that Cargo.lock resolves. If you bump one, bump the other.
# ---------------------------------------------------------------------------
FROM rust:1-bookworm AS build

RUN apt-get update \
 && apt-get install -y --no-install-recommends curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*

ARG CARGO_LEPTOS_VERSION=0.3.7
ARG WASM_BINDGEN_VERSION=0.2.127

RUN rustup target add wasm32-unknown-unknown

# Fetch prebuilt binaries rather than compiling the toolchain from source.
# `cargo install` on these two needs several GB of RAM and many minutes, and
# fails outright on a Docker daemon with a modest memory limit — which is how
# most laptops are configured. cargo-binstall pulls the upstream release
# artifacts instead, so this layer is quick and cheap.
#
# Cached as its own layer: it only changes when the versions above do.
RUN curl -fsSL https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
 && cargo binstall --no-confirm --locked \
      cargo-leptos@${CARGO_LEPTOS_VERSION} \
      wasm-bindgen-cli@${WASM_BINDGEN_VERSION}

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY assets ./assets
COPY style ./style

RUN cargo leptos build --release

# ---------------------------------------------------------------------------
# Runtime
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

# ca-certificates is needed to reach DynamoDB over TLS.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --create-home --shell /usr/sbin/nologin app

WORKDIR /app
COPY --from=build /build/target/release/typing-web /usr/local/bin/typing-web
COPY --from=build /build/target/site ./site
# Practice text is read from disk at startup and served per language.
COPY --from=build /build/assets/klavaro-data ./assets/klavaro-data

ENV LEPTOS_OUTPUT_NAME=typing \
    LEPTOS_SITE_ROOT=/app/site \
    LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:8080 \
    KLAVARO_DATA_DIR=/app/assets/klavaro-data \
    RUST_LOG=info

USER app
EXPOSE 8080

# The load balancer has its own check; this one makes `docker run` honest too.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/usr/local/bin/typing-web", "--health-check"]

CMD ["typing-web"]
