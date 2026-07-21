# syntax=docker/dockerfile:1.7
#
# Runtime serves plain HTTP on port 3000. Terminate TLS at nginx (or another
# reverse proxy) in front of this container; do not mount app certificates here.

FROM oven/bun:1-alpine AS frontend
WORKDIR /app/frontend

COPY frontend/package.json frontend/bun.lock ./
RUN --mount=type=cache,target=/root/.bun/install/cache \
    bun install --frozen-lockfile

COPY frontend/ ./
RUN bun run build

FROM rust:1-alpine AS builder
RUN apk add --no-cache \
    build-base \
    ca-certificates \
    cmake \
    git \
    openssh-client \
    pkgconfig

WORKDIR /app

ENV CARGO_NET_GIT_FETCH_WITH_CLI=true \
    LLM_PROXY_SKIP_FRONTEND_BUILD=1

# Required by the ssh:// Git dependency in Cargo.toml.
RUN mkdir -p -m 0700 /root/.ssh \
    && ssh-keyscan github.com >> /root/.ssh/known_hosts

COPY . .
COPY --from=frontend /app/frontend/dist ./frontend/dist
RUN mkdir -p /runtime-data \
    && cp config.example.json /runtime-data/config.json \
    && chown -R 10001:10001 /runtime-data

RUN --mount=type=ssh,required=true \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked \
    && cp /app/target/release/llm-proxy /usr/local/bin/llm-proxy

# Minimal runtime: HTTP only. ca-certificates is for outbound provider HTTPS,
# not for terminating inbound TLS.
FROM scratch AS runtime

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /usr/local/bin/llm-proxy /usr/local/bin/llm-proxy
COPY --from=builder --chown=10001:10001 /runtime-data /data

USER 10001:10001

ENV HOME=/data \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

# HTTP port only; put nginx (or similar) in front for HTTPS.
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/llm-proxy"]
CMD ["--config", "/data/config.json"]
