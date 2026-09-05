FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates jq \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://raw.githubusercontent.com/gastownhall/beads/main/scripts/install.sh | bash \
    && ln -sf /home/vscode/.local/bin/bd /usr/local/bin/bd
