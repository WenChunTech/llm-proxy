# syntax=docker/dockerfile:1.7
#
# This Dockerfile packages a prebuilt static binary. Build the binary on the
# host first and copy it to docker/<arch>/llm-proxy, where <arch> is Docker's
# TARGETARCH value such as amd64 or arm64.
#
# WORKDIR is /app and owned by the runtime user so relative paths in config
# (for example debug_dump.dir) can create directories at runtime.

FROM alpine:3.22 AS runtime

RUN apk add --no-cache ca-certificates \
    && mkdir -p /app \
    && chown 10001:10001 /app

ARG TARGETARCH=amd64
COPY --chmod=0755 docker/${TARGETARCH}/llm-proxy /usr/local/bin/llm-proxy
COPY --chown=10001:10001 config.example.json /app/config.json

WORKDIR /app
USER 10001:10001

ENV HOME=/app \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/llm-proxy"]
CMD ["--config", "/app/config.json"]
