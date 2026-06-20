# Multi-stage build for the meter engine binary.
# TLS is rustls (no OpenSSL), so the runtime image only needs CA certificates.

FROM rust:1.88-slim-bookworm AS build
WORKDIR /app
COPY . .
RUN cargo build --release -p meter-engine

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/meter /usr/local/bin/meter
EXPOSE 8080
ENV METER_LISTEN_ADDR=0.0.0.0:8080
ENTRYPOINT ["meter"]
