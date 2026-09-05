FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

ARG JUST_VERSION=1.58.0
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates jq make \
    && rm -rf /var/lib/apt/lists/* \
    && curl -fsSL "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
        | tar -xz -C /usr/local/bin just \
    && just --version

RUN su vscode -c 'export PATH="$HOME/.local/bin:$PATH"; curl -fsSL https://raw.githubusercontent.com/gastownhall/beads/main/scripts/install.sh | bash' \
    && ln -sf /home/vscode/.local/bin/bd /usr/local/bin/bd \
    && bd --version
