FROM docker.io/library/rust:slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends systemd \
    && rm -rf /var/lib/apt/lists/*

COPY rust-toolchain.toml /workspace/rust-toolchain.toml
WORKDIR /workspace
RUN rustup toolchain install && rustup component add clippy rustfmt
