# syntax=docker/dockerfile:1.7
#
# This Dockerfile packages a prebuilt static binary. Build the binary on the
# host first and copy it to docker/<arch>/llm-proxy, where <arch> is Docker's
# TARGETARCH value such as amd64 or arm64.

FROM alpine:3.22 AS runtime

RUN apk add --no-cache ca-certificates

ARG TARGETARCH=amd64
COPY --chmod=0755 docker/${TARGETARCH}/llm-proxy /usr/local/bin/llm-proxy
COPY --chown=10001:10001 config.example.json /data/config.json

USER 10001:10001

ENV HOME=/data \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/llm-proxy"]
CMD ["--config", "/data/config.json"]
