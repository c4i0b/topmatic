FROM cgr.dev/chainguard/wolfi-base AS gh
RUN apk add --no-cache gh

FROM docker.io/library/rust:1-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends systemd python3 git curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=gh /usr/bin/gh /usr/local/bin/gh

COPY rust-toolchain.toml /workspace/rust-toolchain.toml
WORKDIR /workspace
RUN rustup toolchain install && rustup component add clippy rustfmt
