# Multi-stage build for cerridwen-server.
#
# Stage 1: builder — installs the Rust toolchain and the C/C++ deps that
# libswisseph-sys needs (libclang for bindgen).
# Stage 2: runtime — minimal Debian slim image with just the binary and
# the Swiss Ephemeris data files.
#
# Build:
#   docker build -t cerridwen .
# Run:
#   docker run -p 2828:2828 cerridwen

FROM rust:1.83-bookworm AS builder

# bindgen needs libclang.
RUN apt-get update && apt-get install -y --no-install-recommends \
        libclang-dev \
        clang \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY rust /build/rust
COPY sweph /build/sweph
COPY chart /build/chart
COPY webapp /build/webapp

WORKDIR /build/rust
RUN cargo build --release --features server,mcp,events --bin cerridwen-server --bin cerridwen-mcp --bin cerridwen-event-generator

# ---------- runtime ----------

FROM debian:bookworm-slim

# Runtime deps: just glibc + a CA bundle (sweph itself is statically linked
# into the binary via libswisseph-sys).
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 1000 -d /app -m cerridwen

WORKDIR /app
COPY --from=builder /build/rust/target/release/cerridwen-server /usr/local/bin/cerridwen-server
COPY --from=builder /build/rust/target/release/cerridwen-mcp /usr/local/bin/cerridwen-mcp
COPY --from=builder /build/rust/target/release/cerridwen-event-generator /usr/local/bin/cerridwen-event-generator
COPY --from=builder /build/sweph /app/sweph

ENV CERRIDWEN_EPHE_PATH=/app/sweph
ENV RUST_LOG=info
USER cerridwen

EXPOSE 2828
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
  CMD ["sh", "-c", "wget -q -O - http://127.0.0.1:2828/health >/dev/null"]

ENTRYPOINT ["cerridwen-server"]
CMD ["--port", "2828"]
