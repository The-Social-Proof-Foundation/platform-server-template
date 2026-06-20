# Build from the ProjectYZ monorepo root so the myso-rust-sdk path dependency resolves:
#   docker build -f platform-server-template/docker/Dockerfile -t platform-server .
FROM rust:1.85-bookworm AS builder

WORKDIR /build
COPY myso-rust-sdk /build/myso-rust-sdk
COPY platform-server-template/Cargo.toml platform-server-template/Cargo.lock /build/platform-server-template/
COPY platform-server-template/crates /build/platform-server-template/crates
COPY platform-server-template/migrations /build/platform-server-template/migrations

WORKDIR /build/platform-server-template
RUN cargo build --release --bin platform-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 1001 platform

WORKDIR /app
COPY --from=builder /build/platform-server-template/target/release/platform-server /usr/local/bin/platform-server
COPY platform-server-template/migrations ./migrations

RUN mkdir -p /app/certs \
    && chown -R platform:platform /app

USER platform
ENV PORT=8080
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
  CMD curl -fsS "http://127.0.0.1:${PORT}/health" || exit 1

CMD ["platform-server"]
